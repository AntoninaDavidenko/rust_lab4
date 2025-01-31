use reqwest::Client;
use serde_json::json;

pub fn get_firebase_api_key() -> &'static str {
    
}

pub async fn register_user(email: &str, password: &str, nickname: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let api_key = get_firebase_api_key();
    let client = Client::new();
    let url = format!(
        "https://identitytoolkit.googleapis.com/v1/accounts:signUp?key={}",
        api_key
    );

    let body = json!({
        "email": email,
        "password": password,
        "displayName": nickname,
        "returnSecureToken": true
    });

    let response = client.post(&url).json(&body).send().await?;
    let status = response.status();
    let text = response.text().await?;
    let data: serde_json::Value = serde_json::from_str(&text)?;

    if status.is_success() {
        let token = data["idToken"].as_str().unwrap_or_default().to_string();
        let local_id = data["localId"].as_str().unwrap_or_default().to_string();
        Ok((token, local_id))
    } else {
        Err(format!("Firebase registration failed: {}", text).into())
    }
}

pub async fn login_user(email: &str, password: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let api_key = get_firebase_api_key();
    let client = Client::new();
    let url = format!(
        "https://identitytoolkit.googleapis.com/v1/accounts:signInWithPassword?key={}",
        api_key
    );

    let body = json!({
        "email": email,
        "password": password,
        "returnSecureToken": true
    });

    let response = client.post(&url).json(&body).send().await?;
    let status = response.status();
    let text = response.text().await?;
    let data: serde_json::Value = serde_json::from_str(&text)?;

    if status.is_success() {
        let token = data["idToken"].as_str().unwrap_or_default().to_string();
        let local_id = data["localId"].as_str().unwrap_or_default().to_string();
        Ok((token, local_id))
    } else {
        Err(format!("Firebase login failed: {}", text).into())
    }
}

pub async fn save_user_to_firestore(
    local_id: &str,
    nickname: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let api_key = get_firebase_api_key();
    let client = Client::new();
    let database_url = format!(
        "https://firestore.googleapis.com/v1/projects/{firebase name}/databases/(default)/documents/users/{}?key={}",
        local_id, api_key
    );

    let body = serde_json::json!({
        "fields": {
            "nickname": { "stringValue": nickname }
        }
    });

    let response = client.patch(&database_url).json(&body).send().await?;
    if response.status().is_success() {
        Ok(())
    } else {
        let error = response.text().await?;
        Err(format!("Failed to save user: {}", error).into())
    }
}


pub async fn get_users_from_firestore() -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let api_key = get_firebase_api_key();
    let client = Client::new();
    let database_url = format!(
        "https://firestore.googleapis.com/v1/projects/{firebase name}/databases/(default)/documents/users?key={}",
        api_key
    );

    let response = client.get(&database_url).send().await?;
    let text = response.text().await?;
    let users_data: serde_json::Value = serde_json::from_str(&text)?;

    let mut users = Vec::new();
    if let Some(documents) = users_data["documents"].as_array() {
        for document in documents {
            let local_id = document["name"].as_str().unwrap_or_default().split('/').last().unwrap_or_default().to_string();
            if let Some(nickname) = document["fields"]["nickname"]["stringValue"].as_str() {
                users.push((local_id, nickname.to_string()));
            }
        }
    }
    Ok(users)
}
