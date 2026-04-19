use std::str::Bytes;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // let initial_post_url = inquire::Text::new("A:").prompt()?;

    let src_file = rfd::FileDialog::new()
        .set_title("Выберите файл с ссылками")
        .pick_file()
        .ok_or("Ошибка выбора файла")?;

    let contents = std::fs::read_to_string(src_file)?;


    for link in contents.split("\n") {
        if link.is_empty() {
            continue;
        }

        // pollster::block_on(get_img_url_from_post_url(client, post_url))
    }



    let initial_post_url = "https://www.youtube.com/post/Ugkx4cdNJgZPYbhSLRehjUHywEwBNlpn7A_f?lc=UgyEejCqC_ilo-zLj_N4AaABAg&si=T7DNr0-442WYtlKS";

    let mut post_url = url::Url::parse(&initial_post_url)?;
    post_url.set_query(None);

    let resp_text = reqwest::blocking::get(post_url)?.text()?;

    let needle = r#"<meta property="og:image" content=""#;
    let start = resp_text.find(needle).ok_or("og:image not found")? + needle.len();
    let rest = &resp_text[start..];
    let end = rest.find("=s").ok_or("FOUND INVALID OG IMAGE ADDRESS")?;

    let final_img_url = format!("{}0", &rest[..(end + 2)]);

    println!("{}", final_img_url);

    let img_resp = reqwest::blocking::get(&final_img_url)?;
    let resp_bytes = img_resp.bytes()?;

    match std::fs::create_dir("obtained") {
        Ok(_) => println!("Создана директория {}", "obtained"),
        Err(e) => eprintln!("Директория наверное уже существует: {}", e),
    }

    std::fs::write("obtained/image.jpg", &resp_bytes)?;

    Ok(())
}

async fn get_img_url_from_post_url(
    client: &reqwest::Client,
    post_url: &url::Url,
) -> Result<String, Box<dyn std::error::Error>> {
    let resp_text = client
        .get(post_url.clone())
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let needle = r#"<meta property="og:image" content=""#;
    let start = resp_text.find(needle).ok_or("og:image not found")? + needle.len();
    let rest = &resp_text[start..];
    let end = rest.find("=s").ok_or("FOUND INVALID OG IMAGE ADDRESS")?;

    Ok(format!("{}0", &rest[..end + 2]))
}


