use thirtyfour::prelude::*;
use std::error::Error;
use std::env;
use std::time::Duration;
use std::collections::HashSet;
use reqwest::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // --- 설정 및 환경변수 로드 ---
    let ia_access_key = env::var("IA_ACCESS_KEY").expect("IA_ACCESS_KEY 환경변수를 설정해주세요! (archive.org 액세스 키)");
    let ia_secret_key = env::var("IA_SECRET_KEY").expect("IA_SECRET_KEY 환경변수를 설정해주세요! (archive.org 시크릿 키)");
    let ia_item_name = "theseed-archive-screenshots-testing";
    let firefox_path = "/home/hgy/firefox-150.0a1.ko.linux-x86_64/firefox/firefox";

    let email = env::var("THESEED_EMAIL").expect("THESEED_EMAIL 환경변수를 설정해주세요!");
    let password = env::var("THESEED_PW").expect("THESEED_PW 환경변수를 설정해주세요!");

    let mut caps = DesiredCapabilities::firefox();
    caps.set_firefox_binary(firefox_path)?;

    let driver = WebDriver::new("http://localhost:4444", caps).await?;
    let http_client = Client::new();

    println!("자동화를 시작합니다.");

    // ==================== 1. 로그인 ====================
    println!("로그인 페이지로 이동 중...");
    driver.goto("https://theseed.io/member/login").await?;
    tokio::time::sleep(Duration::from_secs(3)).await;

    let email_selectors = vec![By::Css("input[name='email']"), By::Css("input[name='username']"), By::Css("input[type='email']")];
    for sel in email_selectors {
        if let Ok(el) = driver.query(sel).first().await {
            el.send_keys(&email).await?;
            println!("이메일 입력 완료");
            break;
        }
    }

    if let Ok(pw_input) = driver.query(By::Name("password")).first().await {
        pw_input.send_keys(&password).await?;
        println!("비밀번호 입력 완료");
    }

    let btn_selectors = vec![By::Css("form button[type='submit']"), By::Css("button[type='submit']"), By::XPath("//button[contains(text(), '로그인')]")];
    for sel in btn_selectors {
        if let Ok(btn) = driver.query(sel).first().await {
            if btn.click().await.is_err() {
                let _ = driver.execute("arguments[0].click();", vec![btn.to_json()?]).await;
            }
            println!("로그인 버튼 클릭 성공");
            break;
        }
    }

    println!("\n[!] 로그인 캡차/Turnstile이 뜨면 브라우저에서 풀고 터미널에서 엔터(Enter)를 눌러주십시오.\n");
    let mut _input = String::new();
    std::io::stdin().read_line(&mut _input)?;

    tokio::time::sleep(Duration::from_secs(1)).await;
    let current_url = driver.current_url().await?.to_string();
    println!("현재 URL: {}", current_url);

    if !current_url.contains("/member/login") {
        println!("로그인에 성공하였습니다");
    } else {
        println!("로그인 실패... 수동으로 로그인 후 엔터 눌러주세요.");
        let mut _input2 = String::new();
        std::io::stdin().read_line(&mut _input2)?;
    }

    // ==================== 2. 문서 수집 ====================
    let search_urls = vec![
        "https://namu.wiki/Search?target=raw&q=%5B%5Bhttps%3A%2F%2Farchive.md%2a&namespace=".to_string(),
        "https://namu.wiki/Search?target=raw&q=%5B%5Bhttps%3A%2F%2Farchive.today%2a&namespace=".to_string(),
        "https://namu.wiki/Search?target=raw&q=%5B%5Bhttps%3A%2F%2Farchive.is%2a&namespace=".to_string(),
        "https://namu.wiki/Search?target=raw&q=%5B%5Bhttps%3A%2F%2Farchive.fo%2a&namespace=".to_string(),
        "https://namu.wiki/Search?target=raw&q=%5B%5Bhttps%3A%2F%2Farchive.li%2a&namespace=".to_string(),
        "https://namu.wiki/Search?target=raw&q=%5B%5Bhttps%3A%2F%2Farchive.vn%2a&namespace=".to_string(),
    ];
    let mut doc_links = HashSet::new();

    for (idx, search_url) in search_urls.iter().enumerate() {
        println!("\n[{}/{}] 검색 URL 처리 중: {}", idx + 1, search_urls.len(), search_url);
        driver.goto(search_url).await?;
        tokio::time::sleep(Duration::from_secs(4)).await;

        for page in 1..=3 {
            println!("  → 페이지 {} 본문 수집 중...", page);

            if let Ok(elements) = driver.find_all(By::Css("section div a[href^='/w/'], [data-v-b9b13aae] a[href^='/w/']")).await {
                for el in elements {
                    if let Ok(Some(href)) = el.attr("href").await {
                        println!("  디버그: 발견된 href = {}", href);
                        if href.starts_with("/w/")
                            && !href.contains("://")
                            && !href.contains("/edit/")
                            && !href.contains("/history/")
                            && !href.contains("/discuss/")
                            && !href.contains("/acl/")
                            && !href.contains("RecentChanges")
                            && !href.contains("Search") {
                            doc_links.insert(format!("https://theseed.io{}", href));
                            println!("  ✅ 추가됨: {}", href);
                        } else {
                            println!("  ❌ 스킵: {}", href);
                        }
                    }
                }
            }

            if page < 3 {
                if let Ok(next_btn) = driver.query(By::Css("a[rel='next'], .next, a[href*='Search?']")).first().await {
                    let _ = next_btn.click().await;
                    tokio::time::sleep(Duration::from_secs(3)).await;
                } else {
                    break;
                }
            }
        }
    }

    println!("본문 검색 결과에서 수집된 모든 문서 개수: {}", doc_links.len());

    // ==================== 3. 각 문서 작업 ====================
    let mut capture_count = 0;
    let archive_domains = vec!["archive.md", "archive.today", "archive.is", "archive.ph", "archive.fo", "archive.li", "archive.vn"];

    for doc_url in doc_links {
        println!("문서 접속: {}", doc_url);
        driver.goto(&doc_url).await?;
        tokio::time::sleep(Duration::from_secs(2)).await;

        let mut replace_map = Vec::new();
        let links = driver.find_all(By::Tag("a")).await?;
        let mut target_urls = Vec::new();

        for link in links {
            if let Ok(Some(href)) = link.attr("href").await {
                if href.starts_with("http") && archive_domains.iter().any(|d| href.contains(d)) {
                    target_urls.push(href);
                }
            }
        }

        for old_url in target_urls {
            println!("아카이브 캡처 중: {}", old_url);
            if driver.goto(&old_url).await.is_err() { continue; }
            tokio::time::sleep(Duration::from_secs(6)).await;

            if let Ok(body) = driver.find(By::Tag("body")).await {
                if let Ok(png) = body.screenshot_as_png().await {
                    capture_count += 1;
                    let filename = format!("theseed_archive_{}.png", capture_count);
                    let upload_url = format!("https://s3.us.archive.org/{}/{}", ia_item_name, filename);
                    let ia_public_url = format!("https://archive.org/download/{}/{}", ia_item_name, filename);

                    let res = http_client.put(&upload_url)
                        .header("Authorization", format!("LOW {}:{}", ia_access_key, ia_secret_key))
                        .header("x-amz-auto-make-bucket", "1")
                        .header("Content-Type", "image/png")
                        .body(png).send().await?;

                    if res.status().is_success() {
                        println!("✅ 업로드 완료: {}", ia_public_url);
                        replace_map.push((old_url, ia_public_url));
                    }
                }
            }
        }

        // ==================== 4. 알림창 처리 및 편집/저장 ====================
        if !replace_map.is_empty() {
            // ==================== 토론 발생 여부 확인 ====================
    println!("편집 전에 사용자 문서에 진행 중인 토론이 있는지 확인중...");
    let target_doc = "사용자:NodeulsumNaru"; // 목표 문서
    let discuss_url = format!("https://theseed.io/discuss/{}", target_doc);
    driver.goto(&discuss_url).await?;

    // 더시드 엔진에서 토론 목록은 보통 table 안에 들어있습니다.
    // '진행 중'인 토론이 있는지 찾기 위해 목록(tr)을 가져옵니다.
    // (스킨이나 테마에 따라 CSS 선택자는 "table tbody tr" 등으로 다를 수 있습니다.)
    if let Ok(threads) = driver.query(By::Css("table.table tbody tr")).all().await {
        if !threads.is_empty() {
            println!("🚨 사용자 문서에 활성화된 토론이 있어 즉시 정지합니다. 🚨");
            driver.quit().await?;
            return Ok(()); // 프로그램 정상 종료
        }
    } else {
        println!("진행 중인 토론이 없습니다.");
    }
            let edit_url = doc_url.replace("/w/", "/edit/");
            driver.goto(&edit_url).await?;
            tokio::time::sleep(Duration::from_secs(3)).await;

            // 캡차/Turnstile 감지
            println!("📢 편집 페이지 로드 중... Turnstile/reCAPTCHA 감지 시도 (최대 60초)");
            for attempt in 0..60 {
                if driver.find(By::Css("iframe[src*='turnstile'], iframe[src*='recaptcha'], .cf-turnstile, .g-recaptcha")).await.is_ok() {
                    println!("Turnstile 또는 reCAPTCHA 감지됨! 브라우저 창에서 직접 풀고 터미널에서 엔터(Enter)를 눌러주세요!");
                    let mut _input = String::new();
                    std::io::stdin().read_line(&mut _input)?;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            println!("📢 알림창 처리 시도 중...");
            for attempt in 0..5 {
                match driver.get_alert_text().await {
                    Ok(text) => {
                        println!("알림창 발견! 내용: {}", text);
                        if text.contains("문서 배포 규정") || text.contains("동의") {
                            driver.accept_alert().await?;
                            println!("✅ 규정 동의 알림창 확인 완료!");
                            tokio::time::sleep(Duration::from_millis(800)).await;
                            break;
                        } else {
                            let _ = driver.dismiss_alert().await;
                        }
                    }
                    Err(_) => { if attempt == 0 { println!("알림창 없음 → 편집 시작"); } break; }
                }
                tokio::time::sleep(Duration::from_millis(400)).await;
            }

            let textarea = driver.query(By::Name("text"))
                .wait(Duration::from_secs(10), Duration::from_millis(500))
                .first()
                .await?;

            let mut content = textarea.prop("value").await?.unwrap_or_default();
            for (old, new) in replace_map {
                content = content.replace(&old, &new);
            }

            driver.execute(
                r#"
                arguments[0].value = arguments[1];
                arguments[0].dispatchEvent(new Event('input', { bubbles: true }));
                "#,
                vec![textarea.to_json()?, content.into()]
            ).await?;

            if let Ok(log) = driver.query(By::Name("log")).first().await {
                log.send_keys("외부 아카이브 링크를 인터넷 아카이브 스크린샷으로 교체").await?;
            }

            // ★★★ 저작권 동의 체크박스 — 상태 감지 후 필요할 때만 클릭! ★★★
            println!("동의 체크박스 상태 확인 중 (저작권 등 동의)...");
            for attempt in 0..3 {
                if let Ok(checkbox) = driver.query(By::Css("input[type='checkbox'][name*='license'], input[type='checkbox'][id*='license'], input[type='checkbox']")).first().await {
                    match checkbox.is_selected().await {
                        Ok(true) => {
                            println!("체크박스가 이미 체크되어 있음 → 클릭 스킵!");
                            break;
                        }
                        Ok(false) => {
                            println!("체크박스가 체크 안 되어 있음 → 클릭 시도!");
                            if checkbox.click().await.is_err() {
                                let _ = driver.execute("arguments[0].click();", vec![checkbox.to_json()?]).await;
                            }
                            println!("동의 체크박스 클릭 완료!");
                            break;
                        }
                        Err(_) => {
                            println!("⚠️ 체크 상태 확인 실패 → 안전하게 클릭 시도");
                            if checkbox.click().await.is_err() {
                                let _ = driver.execute("arguments[0].click();", vec![checkbox.to_json()?]).await;
                            }
                            println!("✅ 동의 체크박스 클릭 완료!");
                            break;
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            // 저장 단계
            println!("저장 버튼 클릭 시도...");
            let save_selectors = vec![
                By::XPath("//button[contains(text(), '저장')]"),
                By::XPath("//button[contains(text(), '편집 요청')]"),
                By::Css("button[type='submit']"),
            ];
            for sel in save_selectors {
                if let Ok(btn) = driver.query(sel).first().await {
                    if btn.click().await.is_err() {
                        let _ = driver.execute("arguments[0].click();", vec![btn.to_json()?]).await;
                    }
                    println!("✅ 저장 버튼 클릭 완료!");
                    break;
                }
            }

            tokio::time::sleep(Duration::from_secs(3)).await;
            if let Ok(_) = driver.get_alert_text().await {
                driver.accept_alert().await?;
                println!("✅ 저장 후 알림창 자동 확인!");
            }

            let final_url = driver.current_url().await?.to_string();
            if !final_url.contains("/edit/") {
                println!("✅ 문서 저장 성공! (URL: {})", final_url);
            } else {
                println!("⚠️ 저장 후 URL이 편집 페이지로 남아있어요.");
            }

            println!("문서 수정 및 저장 완료! 🎉");
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
    driver.quit().await?;
    println!("수고하셨습니다. 모든 작업이 끝났습니다.");
    Ok(())
}
