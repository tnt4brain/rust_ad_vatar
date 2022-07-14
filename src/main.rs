#![feature(proc_macro_hygiene, decl_macro)]

use image::{imageops, GenericImageView, RgbImage};
use ldap3::{LdapConn, Scope, SearchEntry};
use rocket::http::{ContentType, MediaType};
use rocket::{response::content, State};
use std::sync::atomic::{AtomicUsize, Ordering};

#[macro_use]
extern crate rocket;
extern crate core;

struct HitCount {
    count: AtomicUsize,
}

use image::imageops::FilterType;
use rocket::http::ext::IntoCollection;
use std::io::Cursor;

#[get("/")]
fn index(hit_count: State<HitCount>) -> content::Html<String> {
    hit_count.count.fetch_add(1, Ordering::Relaxed);
    return content::Html(format!("Visit count {}", count(hit_count)));
}

#[get("/avatar/jpg/<email>?<size>")]
fn avatar_jpg(email: String, size: u32) -> content::Content<Vec<u8>> {
    let ldap_result = LdapConn::new("ldap://192.168.0.198:389");
    let mut ldap_conn = match ldap_result {
        Ok(ldap_conn) => ldap_conn,
        Err(e) => {
            return content::Content(
                ContentType(MediaType::HTML),
                "Oh, error".as_bytes().to_vec(),
            );
        }
    };
    let bindresult = ldap_conn.simple_bind("cn=frodo,dc=bantu,dc=ru", "secret");
    match bindresult {
        Ok(_br) => (),
        Err(e) => {
            return content::Content(
                ContentType(MediaType::HTML),
                "Oh, error 2".as_bytes().to_vec(),
            );
        }
    }
    let searchres = match ldap_conn.search(
        "ou=Users,dc=bantu,dc=ru",
        Scope::Subtree,
        format!("(&(objectClass=inetOrgPerson)(mail={}))", email).as_str(),
        vec!["jpegPhoto"],
    ) {
        Ok(search) => search.success(),
        Err(_e) => {
            return content::Content(
                ContentType(MediaType::HTML),
                "Oh, search error".as_bytes().to_vec(),
            );
        } /* return format!("Search error: {:?}", e), */
    };
    let (search_res, _err) = searchres.unwrap();
    if let Some(entry) = search_res.into_iter().next() {
        let z = SearchEntry::construct(entry);
        let attr_res = z.bin_attrs.get("jpegPhoto");
        match attr_res {
            Some(res) => {
                if res.len() == 1 {
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
                                    //    return format!("Image encoded OK");
                                }
                                Err(e) => {
                                    content::Content(
                                        ContentType(MediaType::HTML),
                                        (format!("Encoding error {}", e).as_bytes().to_vec()),
                                    )
                                }
                            }
                        }
                        Err(e) => {
                            content::Content(
                                ContentType(MediaType::HTML),
                                (format!("Image loaded with error {}", e))
                                    .as_bytes()
                                    .to_vec(),
                            )
                            // return format!("Image loaded with error {}", e);
                        }
                    };
                } else {
                    return content::Content(
                        ContentType(MediaType::HTML),
                        (format!("Two or more photos, who am I to choose"))
                            .as_bytes()
                            .to_vec(),
                    );
                    // return "Two or more photos, who am I to choose".to_string();
                }
            }
            None => {
                return content::Content(
                    ContentType(MediaType::HTML),
                    (format!("default was here")).as_bytes().to_vec(),
                );
            }
        }
    }
    return content::Content(
        ContentType(MediaType::HTML),
        (format!("OKAY")).as_bytes().to_vec(),
    );
}

#[get("/count")]
fn count(hit_count: State<HitCount>) -> String {
    hit_count.count.load(Ordering::Relaxed).to_string()
}

fn main() {
    rocket::ignite()
        .manage(HitCount {
            count: AtomicUsize::new(0),
        })
        .mount("/", routes![index, avatar_jpg, count])
        .launch();
}
