#![no_std]
mod dto;
extern crate alloc;
use aidoku::{
	error::{AidokuError, AidokuErrorKind, Result},
	helpers::uri::encode_uri,
	prelude::*,
	std::{defaults::defaults_get, net::Request, String, StringRef, Vec},
	Chapter, Filter, FilterType, Listing, Manga, MangaPageResult, Page,
};
use alloc::{vec, borrow::ToOwned, string::ToString};
use dto::{BookDto, PageDto, PageWrapperDto, SeriesDto, ReadListDto};

fn get_authorization_header() -> String {
	let username = defaults_get("username")
		.and_then(|v| v.as_string().map(|v| v.read()))
		.unwrap_or_default();
	let password = defaults_get("password")
		.and_then(|v| v.as_string().map(|v| v.read()))
		.unwrap_or_default();

	let auth = format!("{username}:{password}");

	let authb = auth.as_bytes();
	let mut buf = vec![0; authb.len() * 4 / 3 + 4];
	let len = base64::encode_config_slice(authb, base64::STANDARD, &mut buf);
	buf.resize(len, 0);

	format!("Basic {}", String::from_utf8_lossy(&buf))
}

fn get_base_url() -> Result<String> {
	defaults_get("baseURL")?
		.as_string()
		.map(|v| v.read().trim_end_matches('/').to_string())
}

#[get_manga_list]
fn get_manga_list(filters: Vec<Filter>, page: i32) -> Result<MangaPageResult> {
	let base_url = get_base_url()?;
	let mut url = base_url.clone();
	url.push_str("/api/v1/series?deleted=false&page=");
	url.push_str(itoa::Buffer::new().format(page - 1));
	for filter in filters {
		match filter.kind {
			FilterType::Check => {
				if let Ok(id) = filter.object.get("id").as_string() {
					url.push_str(&id.read());
				}
			}
			FilterType::Sort => {
				if let Ok(value) = filter.value.as_object() {
					let index = value.get("index").as_int().unwrap_or(0);
					let ascending = value.get("ascending").as_bool().unwrap_or(true);
					let property = match index {
						0 => "metadata.titleSort",
						1 => "createdDate",
						2 => "lastModifiedDate",
						_ => continue,
					};
					url.push_str("&sort=");
					url.push_str(property);
					url.push(',');
					url.push_str(if ascending { "asc" } else { "desc" });
				}
			}
			FilterType::Title => {
				if let Ok(title) = filter.value.as_string() {
					let title = title.read();
					if title.starts_with("regex:") {
						url.push_str("&search_regex=");
						url.push_str(title
								.strip_prefix("regex:")
								.map(|v| v.trim())
								.unwrap_or_default(),
						);
						if !title.contains(",TITLE") && !title.contains(",TITLE_SORT") {
							url.push_str(",TITLE");
						}
					} else {
						url.push_str("&search=");
						url.push_str(&title);
					}
				}
			}
			_ => continue,
		}
	}

	let data = Request::get(encode_uri(url))
		.header("Authorization", &get_authorization_header())
		.data();
	serde_json::from_slice(&data)
		.map(|v: PageWrapperDto<SeriesDto>| MangaPageResult {
			manga: v
				.content
				.into_iter()
				.map(|v| v.into_manga(&base_url))
				.collect::<Vec<_>>(),
			has_more: !v.last,
		})
		.map_err(|_| AidokuError {
			reason: AidokuErrorKind::JsonParseError,
		})
}

#[get_manga_listing]
fn get_manga_listing(listing: Listing, page: i32) -> Result<MangaPageResult> {
	let base_url = get_base_url()?;
	let mut url = base_url.clone();
	url.push_str(match listing.name.as_str() {
		"Latest" => "/api/v1/series/latest",
		"New" => "/api/v1/series/new",
		"Updated" => "/api/v1/series/updated",
		"Read lists" => "/api/v1/readlists",
		_ => {
			return Err(AidokuError {
				reason: AidokuErrorKind::Unimplemented,
			})
		}
	});
	url.push_str("?deleted=false&page=");
	url.push_str(itoa::Buffer::new().format(page - 1));

	let data = Request::get(encode_uri(url))
		.header("Authorization", &get_authorization_header())
		.data();

	if listing.name == "Read lists" {
		serde_json::from_slice(&data)
		.map(|v: PageWrapperDto<ReadListDto>| MangaPageResult {
			manga: v
				.content
				.into_iter()
				.map(|v| v.into_manga(&base_url))
				.collect::<Vec<_>>(),
			has_more: !v.last,
		})
		.map_err(|_| AidokuError {
			reason: AidokuErrorKind::JsonParseError,
		})
	}
	else {
		serde_json::from_slice(&data)
		.map(|v: PageWrapperDto<SeriesDto>| MangaPageResult {
			manga: v
				.content
				.into_iter()
				.map(|v| v.into_manga(&base_url))
				.collect::<Vec<_>>(),
			has_more: !v.last,
		})
		.map_err(|_| AidokuError {
			reason: AidokuErrorKind::JsonParseError,
		})
	}
}

#[get_manga_details]
fn get_manga_details(id: String) -> Result<Manga> {
	let base_url = get_base_url()?;
	let is_readlist = id.contains("readlist_");
	let url = if is_readlist {
		let read_list_id = id.replace("readlist_", "");
		format!("{base_url}/api/v1/readlists/{read_list_id}")
	} else  {
		format!("{base_url}/api/v1/series/{id}")
	};

	let data = Request::get(encode_uri(url))
		.header("Authorization", &get_authorization_header())
		.data();

	if is_readlist {
		serde_json::from_slice(&data)
			.map(|v: ReadListDto| v.into_manga(&base_url))
			.map_err(|_| AidokuError {
				reason: AidokuErrorKind::JsonParseError,
			})
	} else {
		serde_json::from_slice(&data)
			.map(|v: SeriesDto| v.into_manga(&base_url))
			.map_err(|_| AidokuError {
				reason: AidokuErrorKind::JsonParseError,
			})
	}
}

#[get_chapter_list]
fn get_chapter_list(id: String) -> Result<Vec<Chapter>> {
	let base_url = get_base_url()?;
	let is_readlist = id.contains("readlist_");
	let url = if is_readlist {
		let read_list_id = id.replace("readlist_", "");
		format!("{base_url}/api/v1/readlists/{read_list_id}/books?unpaged=true&media_status=READY&deleted=false")
	} else {
		format!("{base_url}/api/v1/series/{id}/books?unpaged=true&media_status=READY&deleted=false")
	};
	let data = Request::get(encode_uri(url))
		.header("Authorization", &get_authorization_header())
		.data();
	serde_json::from_slice(&data)
		.map(|v: PageWrapperDto<BookDto>| {
			v.content
				.iter()
				.enumerate()
				.map(|(index, book)| {
					let mut date_updated = book
						.metadata
						.release_date
						.as_ref()
						.map(|v| StringRef::from(v).as_date("yyyy-MM-dd", Some("en_US"), None))
						.unwrap_or(-1.0);
					if date_updated == -1.0 {
						date_updated = StringRef::from(&book.file_last_modified).as_date(
							"yyyy-MM-dd'T'HH:mm:ss",
							Some("en_US"),
							None,
						);
					}
					if date_updated == -1.0 {
						date_updated = StringRef::from(&book.file_last_modified).as_date(
							"yyyy-MM-dd'T'HH:mm:ss'Z",
							Some("en_US"),
							None,
						);
					}
					if date_updated == -1.0 {
						date_updated = StringRef::from(&book.file_last_modified).as_date(
							"yyyy-MM-dd'T'HH:mm:ss.S",
							Some("en_US"),
							None,
						);
					}
					let title = if is_readlist {
						 [book.series_title.clone(), " ".to_owned(), book.metadata.number_sort.to_string(), " - ".to_owned(), book.metadata.title.clone()].concat() 
					} else { 
						book.metadata.title.clone()
					};
					let chapter = if is_readlist {
						(index + 1) as f32
					 } else {
						book.metadata.number_sort
					 };
					Chapter {
						id: book.id.to_owned(),
						url: [&base_url, "/book/", book.id].concat(),
						title,
						chapter,
						date_updated,
						..Default::default()
					}
				})
				.rev()
				.collect::<Vec<_>>()
		})
		.map_err(|_| AidokuError {
			reason: AidokuErrorKind::JsonParseError,
		})
}

#[modify_image_request]
fn modify_image_request(request: Request) {
	request.header("Authorization", &get_authorization_header());
}

#[get_page_list]
fn get_page_list(_: String, id: String) -> Result<Vec<Page>> {
	let base_url = get_base_url()?;
	let url = format!("{base_url}/api/v1/books/{id}/pages");
	let data = Request::get(encode_uri(&url))
		.header("Authorization", &get_authorization_header())
		.data();
	serde_json::from_slice(&data)
		.map(|v: Vec<PageDto>| {
			v.iter()
				.map(|it| {
					let enable_jxl = match defaults_get("enable_jxl") {
						Ok(enable_jxl) => enable_jxl.as_bool().unwrap_or(false),
						Err(_) => false,
					};
					let convert = match it.media_type {
						"image/jpeg" | "image/png" | "image/gif" | "image/webp" => "",
						"image/jxl" if enable_jxl => "",
						_ => "?convert=png",
					};
					
					let page_url = url.clone()
						+ "/" + itoa::Buffer::new().format(it.number)
						+ convert;
					Page {
						index: it.number - 1,
						url: page_url,
						..Default::default()
					}
				})
				.collect::<Vec<_>>()
		})
		.map_err(|_| AidokuError {
			reason: AidokuErrorKind::JsonParseError,
		})
}
