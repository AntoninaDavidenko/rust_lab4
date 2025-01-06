use actix_cors::Cors;
use actix_files::Files;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;
use actix::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

mod firebase;
use firebase::{get_users_from_firestore, login_user, register_user, save_user_to_firestore};

#[derive(Deserialize, Debug)]
struct RegisterPayload {
    email: String,
    password: String,
    nickname: String,
}

#[derive(Deserialize, Debug)]
struct LoginPayload {
    email: String,
    password: String,
}

type ChatState = Arc<Mutex<HashMap<String, Addr<ChatWebSocket>>>>;

struct ChatWebSocket {
    user_id: String,
    peer_id: Option<String>,
    state: ChatState,
}

impl Actor for ChatWebSocket {
    type Context = ws::WebsocketContext<Self>;
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ChatWebSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                if let Some(peer_id) = &self.peer_id {
                    let state = Arc::clone(&self.state);
                    let sender_id = self.user_id.clone();
                    let peer_id = peer_id.clone();
                    let message = format!("{}: {}", sender_id, text);

                    actix::spawn(async move {
                        let state = state.lock().await;
                        if let Some(peer_addr) = state.get(&peer_id) {
                            peer_addr.do_send(ChatMessage(message.clone()));
                        }
                    });
                }
            }
            Ok(ws::Message::Close(_)) => {
                let state = Arc::clone(&self.state);
                let user_id = self.user_id.clone();
                actix::spawn(async move {
                    let mut state = state.lock().await;
                    state.remove(&user_id);
                });
                ctx.stop();
            }
            _ => {}
        }
    }
}

struct ChatMessage(String);

impl Message for ChatMessage {
    type Result = ();
}

impl Handler<ChatMessage> for ChatWebSocket {
    type Result = ();

    fn handle(&mut self, msg: ChatMessage, ctx: &mut Self::Context) {
        ctx.text(msg.0);
    }
}

#[actix_web::get("/ws/{local_id}/{peer_id}")]
async fn websocket_handler(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<(String, String)>,
    state: web::Data<ChatState>,
) -> Result<HttpResponse, actix_web::Error> {
    let (local_id, peer_id) = path.into_inner();
    let state = state.get_ref().clone(); 

    let ws = ChatWebSocket {
        user_id: local_id.clone(),
        peer_id: Some(peer_id),
        state: Arc::clone(&state),
    };

    match ws::start_with_addr(ws, &req, stream) {
        Ok((addr, resp)) => {
            let mut state_guard = state.lock().await;
            state_guard.insert(local_id.clone(), addr); 
            Ok(resp) 
        }
        Err(e) => {
            eprintln!("WebSocket connection error: {:?}", e);
            Err(actix_web::Error::from(e))
        }
    }
}

#[actix_web::post("/register")]
async fn register_handler(payload: web::Json<RegisterPayload>) -> impl Responder {
    match register_user(&payload.email, &payload.password, &payload.nickname).await {
        Ok((token, local_id)) => {
            if let Err(e) = save_user_to_firestore(&local_id, &payload.nickname).await {
                eprintln!("Failed to save user to Firestore: {:?}", e);
            }

            HttpResponse::Ok().json(serde_json::json!({
                "status": "success",
                "token": token,
                "localId": local_id
            }))
        }
        Err(e) => {
            eprintln!("Registration error: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Registration failed: {}", e)
            }))
        }
    }
}

#[actix_web::post("/login")]
async fn login_handler(payload: web::Json<LoginPayload>) -> impl Responder {
    match login_user(&payload.email, &payload.password).await {
        Ok((token, local_id)) => HttpResponse::Ok().json(serde_json::json!({
            "status": "success",
            "token": token,
            "localId": local_id
        })),
        Err(e) => {
            eprintln!("Login error: {:?}", e);
            HttpResponse::Unauthorized().json(serde_json::json!({
                "error": format!("Login failed: {}", e)
            }))
        }
    }
}

#[actix_web::get("/users")]
async fn get_users_handler(req: HttpRequest) -> impl Responder {
    let local_id = req.query_string()
        .split('&')
        .find(|s| s.starts_with("localId="))
        .and_then(|s| s.split('=').nth(1))
        .map(|id| urlencoding::decode(id).unwrap_or_default())
        .unwrap_or_default();

    match get_users_from_firestore().await {
        Ok(users) => {
            let filtered_users: Vec<_> = users
                .into_iter()
                .filter(|(id, _)| id != &local_id)
                .map(|(_, nickname)| nickname)
                .collect();

            HttpResponse::Ok().json(serde_json::json!({ "users": filtered_users }))
        }
        Err(e) => {
            eprintln!("Failed to fetch users from Firestore: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to fetch users: {}", e)
            }))
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let state = web::Data::new(ChatState::new(Mutex::new(HashMap::new())));

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(
                Cors::default()
                    .allowed_origin("http://localhost:8080")
                    .allowed_methods(vec!["GET", "POST", "OPTIONS"])
                    .allowed_headers(vec!["Content-Type", "Authorization"])
                    .supports_credentials(),
            )
            .service(register_handler)
            .service(login_handler)
            .service(get_users_handler)
            .service(websocket_handler)
            .service(Files::new("/", "./static").index_file("index.html"))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
