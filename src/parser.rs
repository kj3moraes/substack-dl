use chrono::{DateTime, FixedOffset};
use core::fmt;
use html2md::parse_html;
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str;

pub struct Parser {
    pub output_dir: String,
    pub original_url: String,
    pub items: Vec<Post>,
}

#[derive(Debug)]
pub enum ParserError {
    SaveItems,
    NoOverwrite,
    CantDelete,
}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ParserError::CantDelete => "Unable to delete directory",
            ParserError::NoOverwrite => "Overwrite not allowed",
            ParserError::SaveItems => "Successfully saved all items",
        };
        write!(f, "{}", s)
    }
}

pub enum SaveStatus {
    Success,
}

impl fmt::Display for SaveStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SaveStatus::Success => "Successfully saved posts",
        };
        write!(f, "{}", s)
    }
}
fn stripslashes(s: &str) -> Option<String> {
    let mut n = String::new();

    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        n.push(match c {
            '\\' => chars.next()?,
            c => c,
        });
    }

    Some(n)
}
impl Parser {
    pub fn new(url: String, output_dir: String) -> Self {
        Parser {
            original_url: url,
            output_dir,
            items: Vec::new(),
        }
    }

    pub async fn fetch_and_parse(&mut self) -> Result<(), Box<dyn Error>> {
        let urls = extract_urls(self.original_url.to_owned()).await?;
        let mut items = Vec::new();
        for url in urls {
            let mut html_str = fetch_html(&url).await?;
            // Find the start and end indices of the JSON string in the HTML
            let start_index = html_str.find("JSON.parse(\"").unwrap() + 12;
            let mut script_subsection = html_str.split_off(start_index);
            // Find the end of the JSON string
            let end_index = script_subsection
                .find(")</script>")
                .or(Some(start_index))
                .unwrap()
                - 1;

            let _ = script_subsection.split_off(end_index);
            let json_str = stripslashes(&script_subsection).unwrap();
            let post: Post = serde_json::from_str(&json_str).unwrap();
            println!("antoehjr deseiralized = {:?}", post);

            let body_html: String = match &post.post {
                None => "none".to_string(),
                Some(d) => {
                    let a = d.body_html.as_ref().unwrap().clone();
                    a
                }
            };
            let md = parse_html(&body_html);
            println!("The converted markdown is {}", md);
            items.push(post);
        }
        self.items = items;
        Ok(())
    }

    // pub fn save_dir_exists(&self) -> bool {
    //     let dir = get_save_dir(&self.output_dir);
    //     does_dir_exist(&dir)
    // }

    // pub fn save_files(self, overwrite: bool) -> Result<SaveStatus, ParserError> {
    //     println!("Saving files in {}", self.output_dir);
    //     let save_dir = get_save_dir(&self.output_dir);

    //     match does_dir_exist(&save_dir) {
    //         true => {
    //             if !overwrite {
    //                 return Err(ParserError::NoOverwrite);
    //             }
    //             delete_dir(&save_dir)?;
    //         }
    //         false => {
    //             fs::create_dir_all(&save_dir).unwrap();
    //         }
    //     }

    //     for it in self.items.iter() {
    //         println!("Write... {}", it.slug().unwrap());
    //         write_file(self.output_dir.as_str(), it)?;
    //     }
    //     Ok(SaveStatus::Success)
    // }
}

// fn get_save_dir(dir: &str) -> PathBuf {
//     let tmp = Path::new("/tmp");
//     let dir_path = tmp.join(dir);
//     dir_path
// }

// fn delete_dir(dir: &PathBuf) -> Result<bool, ParserError> {
//     match fs::remove_dir_all(dir) {
//         Ok(_) => Ok(true),
//         Err(_) => Err(ParserError::CantDelete),
//     }
// }

// fn does_dir_exist(dir: &PathBuf) -> bool {
//     match fs::metadata(dir) {
//         Ok(md) => md.is_dir(),
//         Err(_) => false,
//     }
// }

// fn write_file(dir: &str, item: &Post) -> Result<(), ParserError> {
//     // First save into tmp, and move to provided directory
//     let tmp = Path::new("/tmp");
//     let dir_path = tmp.join(dir);
//     let file_full_path = dir_path.join(item.filename());

//     if !match fs::metadata(&dir_path) {
//         Ok(md) => md.is_dir(),
//         Err(_) => false,
//     } {
//         fs::create_dir_all(&dir_path).unwrap();
//     }

//     println!(
//         "Write file: {}, {}",
//         item.title,
//         String::from(file_full_path.to_str().unwrap())
//     );
//     let mut f = match File::create(&file_full_path) {
//         Ok(file) => file,
//         Err(_e) => return Err(ParserError::SaveItems),
//     };
//     let md_bytes = item.md.as_bytes();
//     match f.write_all(&md_bytes) {
//         Ok(_) => Ok(()),
//         Err(_e) => return Err(ParserError::SaveItems),
//     }
// }

// This function fetches the HTML content of a given URL
async fn fetch_html(url: &str) -> Result<String, Box<dyn Error>> {
    // Create a client using reqwest
    let client = reqwest::Client::new();

    // Send a GET request to the URL
    let res = client.get(url).send().await?;

    // Check if the request was successful and get the text (HTML) from the response
    if res.status().is_success() {
        let body = res.text().await?;
        Ok(body)
    } else {
        Err("Failed to fetch the HTML content".into())
    }
}

async fn extract_urls(feed_url: String) -> Result<Vec<String>, Box<dyn Error>> {
    // get the sitemap from the feed url
    let sitemap_url = feed_url + "sitemap.xml";
    let res = reqwest::get(sitemap_url).await?.bytes().await?;
    let xml_content = String::from_utf8(res.to_vec())?;
    let mut reader = Reader::from_str(&xml_content);
    reader.trim_text(true);

    let mut buf = Vec::new();
    let mut urls = Vec::new();

    // Parse the XML content.
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name() == quick_xml::name::QName(b"loc") => {
                // Detect <loc> start tag and extract text content.
                if let Ok(Event::Text(e)) = reader.read_event_into(&mut buf) {
                    let url = e.unescape()?.into_owned();
                    if url.contains("/p/") {
                        println!("The url is {:?}", url);
                        urls.push(url);
                    }
                }
            }
            Ok(Event::Eof) => break, // Break the loop upon reaching end of file.
            Err(e) => return Err(Box::new(e)), // Return an error if there is a problem in reading XML.
            _ => (),                           // Continue loop for all other events.
        }

        buf.clear(); // Clear the buffer to prepare for the next read_event call.
    }

    Ok(urls)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Details {
    title: Option<String>,
    subtitle: Option<String>,
    canonical_url: Option<String>,
    body_html: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Post {
    base_url: Option<String>,
    post: Option<Details>,
    #[serde(rename = "canonicalUrl")]
    url: String,
}
