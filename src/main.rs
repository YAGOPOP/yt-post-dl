use arboard::Clipboard;
use chrono;
use clap::{Parser, ValueEnum};
use linkify::{LinkFinder, LinkKind};
use reqwest::{Client, header};
use std::collections::HashSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::AsyncWriteExt;
use url::Url;

type ResultAsyncDyn<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

static FILE_COUNTER: AtomicUsize = AtomicUsize::new(1);

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    #[arg(short, long)]
    output: Option<PathBuf>,

    #[arg(short, long, value_enum, conflicts_with = "link")]
    input: Option<InputSource>,

    #[arg(short, long)]
    link: Option<String>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum InputSource {
    #[value(name = "t")]
    Terminal,
    #[value(name = "c")]
    Clipboard,
}

#[tokio::main]
async fn main() -> ResultAsyncDyn<()> {
    let cli = Cli::parse();

    let write_dir = match cli.output {
        Some(d) => d,
        None => PathBuf::from("./obtained"),
    };
    match std::fs::create_dir(&write_dir) {
        Ok(()) => println!("Создана директория: {}", write_dir.display()),
        Err(e) => {
            if !write_dir.is_dir() {
                return Err(format!("{} - не является каталогом.", write_dir.display()).into());
            } else if write_dir.exists() {
            } else {
                return Err(e.into());
            }
        }
    }

    let link_source = match cli.input {
        Some(InputSource::Clipboard) => read_strings_from_clipboard()?,
        Some(InputSource::Terminal) => read_strings_from_terminal()?,
        None => read_strings_from_clipboard()?,
    };
    let links = extract_links(&link_source, sanitize_yt_post_url);

    run(write_dir, links).await?;
    Ok(())
}

async fn run(write_dir: PathBuf, links: HashSet<String>) -> ResultAsyncDyn<()> {
    let client = Client::builder().tls_backend_native().build()?;

    let mut handles = Vec::new();
    for link in links {
        let client = client.clone();
        let write_dir = write_dir.clone();

        handles.push(tokio::spawn(async move {
            dload_imgs_from_post(&link, &client, &write_dir).await
        }));
    }
    for handle in handles {
        handle.await??;
    }

    Ok(())
}

async fn dload_imgs_from_post(
    indirect_url: &str,
    client: &Client,
    write_dir: &Path,
) -> ResultAsyncDyn<()> {
    let resp = client.get(indirect_url).send().await?.error_for_status()?;

    let resp_text = resp.text().await?;

    let img_urls = extract_links(&resp_text, sanitize_ggpht_url);

    for img_url in img_urls {
        file_from_url(&img_url, &client, &write_dir).await?
    }

    Ok(())
}

async fn file_from_url(img_url: &str, client: &Client, write_dir: &Path) -> ResultAsyncDyn<()> {
    println!("Скачивается: {}", &img_url);
    let mut img_response = match client.get(img_url).send().await {
        Ok(r) => match r.error_for_status() {
            Ok(r) => r,
            Err(err) => {
                eprintln!("Пропускаю {img_url}: bad status: {err}");
                return Ok(());
            }
        },
        Err(err) => {
            eprintln!("Пропускаю {img_url}: request failed: {err}");
            return Ok(());
        }
    };

    let filename = format!(
        "image-{}-{}.{}",
        FILE_COUNTER.fetch_add(1, Ordering::Relaxed),
        chrono::Local::now().format("%Y-%m-%d_%H-%M-%S"),
        figure_out_response_file_extension(&img_response.headers())?
    );
    let write_path = write_dir.join(&filename);

    let mut file = tokio::fs::File::create(&write_path).await?;
    while let Some(chunk) = img_response.chunk().await? {
        file.write_all(&chunk).await?;
    }

    println!("Записан файл: {}", &filename);

    Ok(())
}

fn figure_out_response_file_extension(hv: &header::HeaderMap) -> ResultAsyncDyn<&'static str> {
    match hv.get(header::CONTENT_TYPE) {
        Some(t) => match t.to_str()? {
            "image/jpeg" => Ok("jpeg"),
            "image/gif" => Ok("gif"),
            "image/png" => Ok("png"),
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

#[allow(unused)]
fn read_strings_from_terminal() -> Result<String, std::io::Error> {
    let stdin = std::io::stdin();
    let mut result = String::new();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.is_empty() {
            break;
        }
        result.push_str(" ");
        result.push_str(&line);
    }

    Ok(result)
}

fn extract_links(text: &str, sanitize: fn(Url) -> Option<String>) -> HashSet<String> {
    let mut res = HashSet::new();

    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);

    for link in finder.links(text) {
        let raw = link.as_str();
        if let Ok(l) = Url::parse(raw)
            && let Some(link) = sanitize(l)
        {
            res.insert(link);
        }
    }
    
    res
}

fn sanitize_yt_post_url(mut url: Url) -> Option<String> {
    let host = url.host_str()?;
    if !is_domain_or_subdomain(host, "youtube.com") {
        return None;
    }
    if url.path_segments()?.next() != Some("post") {
        return None;
    }
    url.set_query(None);

    Some(url.as_str().to_owned())
}

fn sanitize_ggpht_url(url: Url) -> Option<String> {
    if url.host_str()? != "yt3.ggpht.com" {
        return None;
    }
    let str_url = url.as_str();
    let i = str_url.find("=")?;

    Some(format!("{}s0", &str_url[..=i]))
}

fn is_domain_or_subdomain(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn read_strings_from_clipboard() -> Result<String, arboard::Error> {
    let mut clpbrd = Clipboard::new()?;
    let text = clpbrd.get_text()?;
    println!("Прочитан текст из буфера обмена:\n{}\n", &text);
    Ok(text)
}
