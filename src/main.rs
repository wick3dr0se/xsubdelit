use std::{env, error::Error, fs::File, io::Write};
use dotenv::dotenv;
use reqwest::Client;
use serde_json::{json, to_string_pretty, Value};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();

    let user_agent = env::var("USER_AGENT")?;
    let client_id = env::var("CLIENT_ID")?;
    let client_secret = env::var("CLIENT_SECRET")?;
    let username = env::var("USERNAME")?;
    let password = env::var("PASSWORD")?;

    let client = Client::new();
    
    let auth_resp: Value = client
        .post("https://www.reddit.com/api/v1/access_token")
        .basic_auth(client_id, Some(client_secret))
        .form(&[
            ("grant_type", "password"),
            ("username", &username),
            ("password", &password)
        ])
        .header("User-Agent", &user_agent)
        .send()
        .await?
        .json()
        .await?;

    let access_token = auth_resp["access_token"].as_str().unwrap();    
    
    let mut after = Some(String::new());
    let mut subscribed_subreddits = Vec::new();

    while let Some(after_token) = after {
        let subreddit_resp: Value = client
            .get("https://oauth.reddit.com/subreddits/mine/subscriber")
            .bearer_auth(access_token)
            .header("User-Agent", &user_agent)
            .query(&[("after", &after_token)])
            .send()
            .await?
            .json()
            .await?;

        let subreddits = subreddit_resp["data"]["children"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|child| {
                child["data"]["display_name"].as_str().map(|s| s.to_string())
            })
            .collect::<Vec<String>>();

        subscribed_subreddits.extend(subreddits);

        after = subreddit_resp["data"]["after"].as_str().map(|s| s.to_string());
    }

    let mut after = Some(String::new());
    let mut file = File::create("comments.json")?;

    while let Some(after_token) = after {
        let comments_resp: Value = client
            .get(&format!(
                "https://oauth.reddit.com/user/{}/comments?after={}",
                username, after_token
            ))
            .bearer_auth(access_token)
            .header("User-Agent", &user_agent)
            .send()
            .await?
            .json()
            .await?;

        let comments = &comments_resp["data"]["children"];

        for comment in comments.as_array().unwrap() {
            let subreddit = comment["data"]["subreddit"].as_str().unwrap();

            println!("Checking comment from subreddit: {}", subreddit);

            if !subscribed_subreddits.contains(&subreddit.to_string()) {
                let comment_id = comment["data"]["id"].as_str().unwrap();
                let timestamp = comment["data"]["created_utc"].as_f64().unwrap();
                let body = comment["data"]["body"].as_str().unwrap_or("");

                println!("Deleting & logging comment: {} from subreddit {}", comment_id, subreddit);
        
                let comment_data = to_string_pretty(&json!({
                    "comment_id": comment_id,
                    "subreddit": subreddit,
                    "timestamp": timestamp,
                    "body": body
                }))?;

                file.write(comment_data.as_bytes())?;

                let delete_resp = client
                    .post("https://oauth.reddit.com/api/del")
                    .bearer_auth(access_token)
                    .form(&[("id", format!("t1_{}", comment_id))])
                    .header("User-Agent", &user_agent)
                    .send()
                    .await?;

                if delete_resp.status().is_success() {
                    println!("Successfully deleted comment: {}", comment_id);
                } else {
                    println!("Failed to delete comment: {}. Error: {}", comment_id, delete_resp.text().await?);
                }
            }
        }

        after = comments_resp["data"]["after"].as_str().map(|s| s.to_string());
    }

    Ok(())
}