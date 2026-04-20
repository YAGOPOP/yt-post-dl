use std::io::BufRead;
use std::sync::atomic::{AtomicUsize, Ordering};
// use std::str::FromStr;
use chrono;

use reqwest::{Client, header};
use url::Url;

type ResultAsyncDyn<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

static FILE_COUNTER: AtomicUsize = AtomicUsize::new(1);

#[tokio::main]
async fn main() -> ResultAsyncDyn<()> {

    let client = Client::builder().tls_backend_native().build()?;
    let write_dir = std::path::PathBuf::from("./obtained");

    let mut handles = Vec::new();
    for link in obtain_links()? {
        handles.push(tokio::spawn({
            file_from_indirect_url_own(link, client.clone(), write_dir.clone())
        }));
    }

    for handle in handles {
        handle.await??;
    }

    Ok(())
}

const NEEDLE: &str = r#"<meta property="og:image" content=""#;
fn extract_file_url(body: &str) -> ResultAsyncDyn<String> {
    let start = body.find(NEEDLE).ok_or("og:image not found")? + NEEDLE.len();
    let rest = &body[start..];
    let end = rest.find("=s").ok_or("FOUND INVALID OG IMAGE ADDRESS")?;

    Ok(format!("{}0", &rest[..(end + 2)]))
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

    let dirty_img_url = extract_file_url(&resp_text)?;
    let img_url = prep_link(&dirty_img_url)?;

    let img_response = client.get(img_url).send().await?;

    let filename = format!(
        "image-{}-{}.{}",
        FILE_COUNTER.fetch_add(1, Ordering::Relaxed),
        chrono::Local::now().format("%Y-%m-%d_%H-%M-%S"),
        figure_out_response_file_extension(&img_response.headers())?
    );
    let write_parh = write_dir.join(&filename);

    let img_bytes = img_response.bytes().await?;
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

fn read_strings() -> ResultAsyncDyn<Vec<String>> {
    let stdin = std::io::stdin();

    Ok(stdin.lock().lines().collect::<Result<Vec<_>, _>>()?)
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
    let lines = read_strings()?;
    Ok(extract_links(&lines))
}
