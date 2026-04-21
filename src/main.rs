use chrono;
use regex::Regex;
use reqwest::{Client, header};
use std::collections::HashSet;
use std::io::BufRead;
use std::sync::atomic::{AtomicUsize, Ordering};
use url::Url;
use arboard::Clipboard;

type ResultAsyncDyn<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

static FILE_COUNTER: AtomicUsize = AtomicUsize::new(1);

#[tokio::main]
async fn main() -> ResultAsyncDyn<()> {
    let client = Client::builder().tls_backend_native().build()?;
    let write_dir = std::path::PathBuf::from("./obtained");

    let links = obtain_links()?;
    println!("Ввод обработан, скачивание...");
    let mut handles = Vec::new();
    for link in links {
        handles.push(tokio::spawn({
            file_from_indirect_url_own(link, client.clone(), write_dir.clone())
        }));
    }

    for handle in handles {
        handle.await??;
    }

    Ok(())
}

fn prep_link(raw_link: &str) -> ResultAsyncDyn<Url> {
    let mut link = Url::parse(raw_link)?;
    link.set_query(None);
    Ok(link)
}

async fn file_from_indirect_url_own(
    indirect_url: String,
    client: Client,
    write_dir: std::path::PathBuf,
) -> ResultAsyncDyn<()> {
    file_from_indirect_url(&indirect_url, &client, &write_dir).await
}

async fn file_from_indirect_url(
    indirect_url: &str,
    client: &Client,
    write_dir: &std::path::PathBuf,
) -> ResultAsyncDyn<()> {
    let link = prep_link(indirect_url)?;
    let resp = client.get(link).send().await?;

    let resp_text = resp.text().await?;

    let img_urls = extract_all_ggpht_urls(&resp_text);

    for img_url in img_urls {
        println!("Скачивается: {}", &img_url);
        file_from_url(&img_url, &client, &write_dir).await?
    }

    Ok(())
}

async fn file_from_url(
    img_url: &str,
    client: &Client,
    write_dir: &std::path::PathBuf,
) -> ResultAsyncDyn<()> {
    let img_response = match client.get(img_url).send().await {
        Ok(r) => r,
        Err(err) => {
            eprintln!("Пропускаю {img_url}: extract_file_url failed: {err}");
            return Ok(());
        }
    };

    let filename = format!(
        "image-{}-{}.{}",
        FILE_COUNTER.fetch_add(1, Ordering::Relaxed),
        chrono::Local::now().format("%Y-%m-%d_%H-%M-%S"),
        figure_out_response_file_extension(&img_response.headers())?
    );

    let img_bytes = img_response.bytes().await?;

    let write_parh = write_dir.join(&filename);
    tokio::fs::write(&write_parh, &img_bytes).await?;
    println!("Записан файл: {}", &filename);

    Ok(())
}

fn figure_out_response_file_extension(hv: &header::HeaderMap) -> ResultAsyncDyn<String> {
    match hv.get(header::CONTENT_TYPE) {
        Some(t) => match t.to_str()? {
            "image/jpeg" => Ok("jpeg".to_owned()),
            "image/gif" => Ok("gif".to_owned()),
            "image/png" => Ok("png".to_owned()),
            ut => {
                return Err(
                    format!("Ошибка: не предусмотренный тип контента в ответе: {}", ut).into(),
                );
            }
        },
        None => {
            return Err(
                "Ошибка: в ответе от сервера на запрос по прямой ссылке картинки нет контента."
                    .into(),
            );
        }
    }
}

// fn read_strings() -> ResultAsyncDyn<Vec<String>> {
//     let stdin = std::io::stdin();

//     Ok(stdin.lock().lines().collect::<Result<Vec<_>, _>>()?)
// }

#[allow(unused)]
fn read_strings() -> Result<Vec<String>, std::io::Error> {
    let stdin = std::io::stdin();
    let mut result = Vec::new();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.is_empty() {
            break;
        }
        result.push(line);
    }

    Ok(result)
}

fn extract_links(lines: &[String]) -> Vec<String> {
    lines.iter().filter_map(|line| extract_link(line)).collect()
}

fn extract_link(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|part| part.starts_with("http://") || part.starts_with("https://"))
        .map(str::to_string)
}

fn obtain_links() -> ResultAsyncDyn<Vec<String>> {
    // let lines = read_strings()?;
    let lines = read_strings_from_clipboard()?;
    Ok(extract_links(&lines))
}

fn extract_all_ggpht_urls(body: &str) -> HashSet<String> {
    let re = Regex::new(r#"https://yt3\.ggpht\.com/[^"'<>\s\\=]+="#).unwrap();

    let mut out = HashSet::new();
    for m in re.find_iter(body) {
        let url = format!("{}s0", m.as_str());
        out.insert(url);
    }

    out
}


fn read_strings_from_clipboard() -> Result<Vec<String>, arboard::Error> {
    let mut clpbrd = Clipboard::new()?;
    let text = clpbrd.get_text()?;

    let mut res = Vec::new();
    for line in text.split("\n") {
        res.push(line.to_owned());
    }
    Ok(res)
}
