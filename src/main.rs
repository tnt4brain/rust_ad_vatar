use image::imageops::FilterType;
use image::{EncodableLayout, GenericImageView};
use ldap3::{LdapConnAsync, Scope, SearchEntry};
use rocket::http::ContentType;
use rocket::http::Status;
use rocket::Either::{Left, Right};
use rocket::{Either, State};
use std::env;
use std::fs;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};

#[macro_use]
extern crate rocket;
extern crate core;
extern crate image;
extern crate ldap3;

const ERR_CONNECTION: &str = "Connection error";
const ERR_BIND: &str = "Binding as bind_dn error";
const ERR_SEARCH: &str = "Object search error";

struct HitCount {
    count: AtomicUsize,
}

struct ServerConfig {
    ldap_uri: String,
    bind_dn: String,
    bind_pw: String,
    base: String,
    attr: String,
    file_static: Vec<u8>,
}

fn return_default_image(filename: &str) -> Vec<u8> {
    let f: &str = if filename.len() > 0 {
        filename
    } else {
        "default.png"
    };
    fs::read(f).unwrap()
}

#[get("/")]
async fn index(hit_count: &State<HitCount>) -> &str {
    hit_count.count.fetch_add(1, Ordering::Relaxed);
    "OK"
}

#[get("/metrics")]
async fn vizit_count(hit_count: &State<HitCount>) -> (Status, (ContentType, String)) {
    (
        Status::Ok,
        (
            ContentType::Text,
            format!(
                "# TYPE http_server_requests_total counter
# HELP http_server_requests_total The total number of HTTP requests handled by Rocket application
http_server_requests_total {}",
                hit_count.count.load(Ordering::Relaxed)
            ),
        ),
    )
}

#[get("/default")]
async fn default_image<'a>(
    hit_count: &'a State<HitCount>,
    config: &'a State<ServerConfig>,
) -> (ContentType, &'a [u8]) {
    hit_count.count.fetch_add(1, Ordering::Relaxed);
    (ContentType::PNG, &config.file_static)
}

async fn read_ldap(
    config: &State<ServerConfig>,
    email: String,
    size: u32,
) -> Either<Box<[u8]>, String> {
    let ldap_uri = config.ldap_uri.clone();
    let tmp_conn = LdapConnAsync::new(&ldap_uri.to_owned()).await;
    let mut ldap_handle = match tmp_conn {
        Ok(tmp_conn) => {
            ldap3::drive!(tmp_conn.0);
            tmp_conn.1
        }
        Err(e) => {
            return Right(format!("{0}: {1}", ERR_CONNECTION, e.to_string()).to_string());
        }
    };
    let _ = match ldap_handle
        .simple_bind(&config.bind_dn, &config.bind_pw)
        .await
    {
        Ok(_) => {}
        Err(e) => {
            return Right(format!("{0}: {1}", ERR_BIND, e.to_string()).to_string());
        }
    };
    let search_result = match ldap_handle
        .search(
            &config.base,
            Scope::Subtree,
            format!("(&(objectClass=inetOrgPerson)(mail={}))", email).as_str(),
            vec![&config.attr],
        )
        .await
    {
        Ok(search) => search.success().unwrap(),
        Err(e) => {
            return Right(format!("{0}: {1}", ERR_SEARCH, e.to_string()).to_string());
        }
    };
    if let Some(entry) = search_result.0.iter().next() {
        match SearchEntry::construct(entry.clone())
            .bin_attrs
            .get(&config.attr)
        {
            Some(search_item) => {
                let ldap_image = image::load_from_memory(&search_item[0]);
                match ldap_image {
                    Ok(img) => {
                        let (width, height) = img.dimensions();
                        let ret_height = if width >= height { height } else { width };
                        let ret_width = if width < height { width } else { height };
                        let target_size = match (size < 32) || (size > 512) {
                            false => size,
                            true => 64,
                        };

                        let cropped_img = img.crop_imm(0, 0, ret_width, ret_height).resize(
                            target_size,
                            target_size,
                            FilterType::Lanczos3,
                        );

                        let mut img_output: Vec<u8> = Vec::new();
                        match cropped_img
                            .write_to(&mut Cursor::new(&mut img_output), image::ImageFormat::Jpeg)
                        {
                            Ok(_image) => Left(img_output.as_bytes().into()),
                            Err(e) => Right(e.to_string()),
                        }
                    }
                    Err(e) => Right(e.to_string()),
                }
            }
            None => Left(config.file_static.as_bytes().into()),
        }
    } else {
        Left(config.file_static.as_bytes().into())
    }
}

#[get("/avatar/<email>?<s>")]
async fn avatar_jpg(
    email: &str,
    s: u32,
    hit_count: &State<HitCount>,
    config: &State<ServerConfig>,
) -> (Status, (ContentType, Box<[u8]>)) {
    hit_count.count.fetch_add(1, Ordering::Relaxed);
    // let size =
    //     match size_param.parse::<u32>() {
    //         Ok(n) => n,
    //         Err(e) => 128
    //     };
    let result_tuple = {
        let ldap_result = read_ldap(config, email.to_string(), s).await;
        if ldap_result.is_left() {
            (Status::Ok, (ContentType::JPEG, ldap_result.left().unwrap()))
        } else if ldap_result.is_right() {
            (
                Status::InternalServerError,
                (
                    ContentType::Text,
                    ldap_result
                        .right()
                        .unwrap()
                        .to_owned()
                        .into_boxed_str()
                        .into_boxed_bytes(),
                ),
            )
        } else {
            (
                Status::InternalServerError,
                (
                    ContentType::Text,
                    "Unexpected error"
                        .to_owned()
                        .into_boxed_str()
                        .into_boxed_bytes(),
                ),
            )
        }
    };
    result_tuple
}

#[launch]
fn rocket() -> _ {
    let ldap_config = ServerConfig {
        ldap_uri: env::var("RA_LDAP_URI").unwrap_or_else(|_e| "ldap://127.0.0.1:389".to_string()),
        bind_dn: env::var("RA_BIND_DN")
            .unwrap_or_else(|_e| "cn=bind_account,dc=acme,dc=com".to_string()),
        bind_pw: env::var("RA_BIND_PASSWORD").unwrap_or_else(|_e| "$3cr3tp4$$w0rd".to_string()),
        base: env::var("RA_SEARCH_BASE").unwrap_or_else(|_e| "ou=users,dc=acme,dc=com".to_string()),
        attr: env::var("RA_IMAGE_ATTR").unwrap_or_else(|_e| "thumbnailPhoto".to_string()),
        file_static: return_default_image(""),
    };
    rocket::build()
        .manage(HitCount {
            count: AtomicUsize::new(0),
        })
        .manage(ldap_config)
        .mount("/", routes![index, vizit_count, default_image, avatar_jpg])
}
