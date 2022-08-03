#![feature(proc_macro_hygiene, decl_macro)]

use image::{GenericImageView};
use ldap3::{LdapConn, Scope, SearchEntry};
use rocket::http::{ContentType, MediaType};
use rocket::{response::content, State};
use std::sync::atomic::{AtomicUsize, Ordering};

#[macro_use]
extern crate rocket;
extern crate core;

const ERR_CONNECTION: &str = "Connection error";
const ERR_BIND: &str = "Binding as bind_dn error";
const ERR_SEARCH: &str = "Object search error";
const ERR_ENCODING: &str = "Image encoding error";
const ERR_RETRIEVAL: &str = "LDAP image retrieval error";

struct HitCount {
    count: AtomicUsize,
}

struct ServerConfig {
    ldap_uri: String,
    bind_dn: String,
    bind_pw: String,
    base: String,
    attr: String,
}

use image::imageops::FilterType;
use std::io::Cursor;
use std::env;

#[get("/")]
fn index(hit_count: State<HitCount>) -> content::Html<String> {
    hit_count.count.fetch_add(1, Ordering::Relaxed);
    return content::Html(format!("Visit count {}", count(hit_count)));
}

#[get("/avatar/jpg/<email>?<size>")]
fn avatar_jpg(email: String, size: u32, config: State<ServerConfig>) -> content::Content<Vec<u8>> {
    let ldap_result = LdapConn::new(&config.ldap_uri);
    let mut ldap_conn = match ldap_result {
        Ok(ldap_conn) => ldap_conn,
        Err(e) => {
            return content::Content(
                ContentType(MediaType::HTML),
                ERR_CONNECTION.as_bytes().to_vec(),
            );
        }
    };
    let bindresult = ldap_conn.simple_bind(&config.bind_dn, &config.bind_pw);
    match bindresult {
        Ok(_br) => (),
        Err(e) => {
            return content::Content(
                ContentType(MediaType::HTML),
                ERR_BIND.as_bytes().to_vec(),
            );
        }
    }
    let searchres = match ldap_conn.search(
        &config.base,
        Scope::Subtree,
        format!("(&(objectClass=inetOrgPerson)(mail={}))", email).as_str(),
        vec![&config.attr],
    ) {
        Ok(search) => search.success(),
        Err(_e) => {
            return content::Content(
                ContentType(MediaType::HTML),
                ERR_SEARCH.as_bytes().to_vec(),
            );
        } /* return format!("Search error: {:?}", e), */
    };
    let (search_res, _err) = searchres.unwrap();
    if let Some(entry) = search_res.into_iter().next() {
        let z = SearchEntry::construct(entry);
        let attr_res = z.bin_attrs.get(&config.attr);
        match attr_res {
            Some(res) => {
                let arr: &[u8] = &res[0];
                let img = image::load_from_memory(arr);
                return match img {
                    Ok(img) => {
                        let (width, height) = img.dimensions();
                        let ret_height = if width >= height { height } else { width };
                        let ret_width = if width < height { width } else { height };
                        let target_size = match (size <= 32) || (size >= 512) {
                            false => size,
                            true => 32,
                        };

                        let cropped_img = img.crop_imm(0, 0, ret_width, ret_height).resize(
                            target_size,
                            target_size,
                            FilterType::CatmullRom,
                        );

                        let mut img_output: Vec<u8> = Vec::new();
                        let write_result = cropped_img.write_to(
                            &mut Cursor::new(&mut img_output),
                            image::ImageOutputFormat::Jpeg(100),
                        );
                        match write_result {
                            Ok(image) => {
                                content::Content(
                                    ContentType(MediaType::JPEG),
                                    img_output,
                                )
                            }
                            Err(e) => {
                                content::Content(
                                    ContentType(MediaType::HTML),
                                    ERR_ENCODING.as_bytes().to_vec(),
                                )
                            }
                        }
                    }
                    Err(e) => {
                        content::Content(
                            ContentType(MediaType::HTML),
                            ERR_RETRIEVAL.as_bytes().to_vec(),
                        )
                    }
                };
            }
            None => {
                return_default_image()
            }
        }
    } else {
        return_default_image()
    }
}

fn return_default_image() -> content::Content<Vec<u8>> {
    let def_img = image::open("default.png");

    // let img_output =  Vec::new();
    // let write_result = def_img.write_to(
    //     &mut Cursor::new(&img_output),
    //     image::ImageOutputFormat::Jpeg(100),
    // );

    match def_img {
        Ok(image) => {
            content::Content(
                ContentType(MediaType::PNG),
                image.into_bytes()
            )
        }
        Err(e) => {
            content::Content(
                ContentType(MediaType::HTML),
                ERR_ENCODING.as_bytes().to_vec(),
            )
        }
    }
}

#[get("/count")]
fn count(hit_count: State<HitCount>) -> String {
    hit_count.count.load(Ordering::Relaxed).to_string()
}

fn main() {
    let config = ServerConfig {
        ldap_uri: match env::var("RA_LDAP_URI") {
            Ok(val) => val,
            Err(e) => "ldap://127.0.0.1:389".to_string()
        },
        bind_dn: match env::var("RA_BIND_DN") {
            Ok(val) => val,
            Err(e) => "cn=frodo,dc=bantu,dc=ru".to_string()
        },
        bind_pw: match env::var("RA_BIND_PASSWORD") {
            Ok(val) => val,
            Err(e) => "secret".to_string()
        },
        base: match env::var("RA_SEARCH_BASE") {
            Ok(val) => val,
            Err(e) => "ou=Users,dc=bantu,dc=ru".to_string()
        },
        attr: match env::var("RA_IMAGE_ATTR") {
            Ok(val) => val,
            Err(e) => "jpegPhoto".to_string()
        },
    };

    rocket::ignite()
        .manage(HitCount {
            count: AtomicUsize::new(0),
        }
        )
        .manage(config)
        .mount("/", routes![index, avatar_jpg, count])
        .launch();
}
