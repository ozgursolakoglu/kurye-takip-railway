use axum::{extract::{Path, State}, http::{HeaderMap, StatusCode}, routing::{get, post}, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::{env, sync::Arc};
use uuid::Uuid;
use argon2::{Argon2, PasswordHash, PasswordVerifier, PasswordHasher, password_hash::{SaltString, rand_core::OsRng}};
use sha2::{Digest, Sha256};
use rand::RngCore;
type ApiResult<T> = Result<Json<T>, (StatusCode, Json<Value>)>;
fn err(code: StatusCode, message: &str) -> (StatusCode, Json<Value>) { (code, Json(json!({"error":message}))) }
fn bad(msg:&str)->(StatusCode,Json<Value>){err(StatusCode::BAD_REQUEST,msg)}
fn internal()->(StatusCode,Json<Value>){err(StatusCode::INTERNAL_SERVER_ERROR,"Database error")}
#[derive(Clone)] struct App { db: PgPool }
#[derive(Deserialize)] struct Login { identifier: String, password: String }
#[derive(Deserialize)] struct NewUser { full_name: String, email: Option<String>, phone_e164: Option<String>, password: String, role: String }
#[derive(Deserialize)] struct NewReceipt { receipt_number: String, receipt_date: chrono::NaiveDate, payment_method: String }
#[derive(Deserialize)] struct Cancel { reason: String }
#[derive(Serialize)] struct User { id: Uuid, full_name: String, role: String }
fn token_hash(token:&str)->String { format!("{:x}",Sha256::digest(token.as_bytes())) }
async fn actor(db:&PgPool, headers:&HeaderMap)->Result<User,(StatusCode,Json<Value>)>{
 let token=headers.get("authorization").and_then(|v|v.to_str().ok()).and_then(|v|v.strip_prefix("Bearer ")).ok_or_else(||err(StatusCode::UNAUTHORIZED,"Authentication required"))?;
 let row=sqlx::query("SELECT u.id,u.full_name,u.role FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=$1 AND s.expires_at>now() AND u.active=true")
 .bind(token_hash(token)).fetch_optional(db).await.map_err(|_|internal())?.ok_or_else(||err(StatusCode::UNAUTHORIZED,"Session expired"))?;
 Ok(User{id:row.get("id"),full_name:row.get("full_name"),role:row.get("role")})
}
fn manager(user:&User)->Result<(),(StatusCode,Json<Value>)>{if user.role=="manager"{Ok(())}else{Err(err(StatusCode::FORBIDDEN,"Manager access required"))}}
async fn health()->Json<Value>{Json(json!({"status":"ok"}))}
async fn login(State(app):State<Arc<App>>,Json(input):Json<Login>)->ApiResult<Value>{
 let identifier=input.identifier.trim().to_lowercase();
 let row=sqlx::query("SELECT id, password_hash FROM users WHERE active=true AND (lower(email)=$1 OR phone_e164=$1)")
 .bind(&identifier).fetch_optional(&app.db).await.map_err(|_|internal())?.ok_or_else(||err(StatusCode::UNAUTHORIZED,"Invalid credentials"))?;
 let hash:String=row.get("password_hash");
 let parsed=PasswordHash::new(&hash).map_err(|_|internal())?;
 Argon2::default().verify_password(input.password.as_bytes(),&parsed).map_err(|_|err(StatusCode::UNAUTHORIZED,"Invalid credentials"))?;
 let mut bytes=[0u8;32];rand::rngs::OsRng.fill_bytes(&mut bytes);
 let token=hex::encode(bytes);
 sqlx::query("INSERT INTO sessions(token_hash,user_id,expires_at) VALUES($1,$2,now()+interval '7 days')")
 .bind(token_hash(&token)).bind(row.get::<Uuid,_>("id")).execute(&app.db).await.map_err(|_|internal())?;
 Ok(Json(json!({"token":token})))
}
async fn me(State(app):State<Arc<App>>,headers:HeaderMap)->ApiResult<User>{Ok(Json(actor(&app.db,&headers).await?))}
async fn create_user(State(app):State<Arc<App>>,headers:HeaderMap,Json(input):Json<NewUser>)->ApiResult<Value>{
 manager(&actor(&app.db,&headers).await?)?;
 if !["manager","courier"].contains(&input.role.as_str()) || input.full_name.trim().is_empty() || input.password.len()<12 {return Err(bad("Invalid role, name, or password (min 12 characters)"))}
 let email=input.email.map(|x|x.trim().to_lowercase()).filter(|x|!x.is_empty());
 let phone=input.phone_e164.map(|x|x.trim().to_string()).filter(|x|!x.is_empty());
 if email.is_none() && phone.is_none(){return Err(bad("Email or phone required"))}
 let hash=Argon2::default().hash_password(input.password.as_bytes(),&SaltString::generate(&mut OsRng)).map_err(|_|internal())?.to_string();
 let id=sqlx::query_scalar::<_,Uuid>("INSERT INTO users(full_name,email,phone_e164,password_hash,role) VALUES($1,$2,$3,$4,$5) RETURNING id")
 .bind(input.full_name.trim()).bind(email).bind(phone).bind(hash).bind(input.role).fetch_one(&app.db).await.map_err(|_|bad("Invalid or duplicate contact"))?;
 Ok(Json(json!({"id":id})))
}
async fn users(State(app):State<Arc<App>>,headers:HeaderMap)->ApiResult<Value>{
 manager(&actor(&app.db,&headers).await?)?;
 let rows=sqlx::query("SELECT id,full_name,email,phone_e164,role,active FROM users ORDER BY full_name").fetch_all(&app.db).await.map_err(|_|internal())?;
 Ok(Json(Value::Array(rows.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"name":r.get::<String,_>("full_name"),"email":r.get::<Option<String>,_>("email"),"phone":r.get::<Option<String>,_>("phone_e164"),"role":r.get::<String,_>("role"),"active":r.get::<bool,_>("active")})).collect())))
}
async fn create_receipt(State(app):State<Arc<App>>,headers:HeaderMap,Json(input):Json<NewReceipt>)->ApiResult<Value>{
 let user=actor(&app.db,&headers).await?;
 if user.role!="courier"{return Err(err(StatusCode::FORBIDDEN,"Courier access required"))}
 if input.receipt_number.trim().is_empty() || !["cash","card","meal","iban"].contains(&input.payment_method.as_str()){return Err(bad("Invalid receipt"))}
 let mut tx=app.db.begin().await.map_err(|_|internal())?;
 let id=sqlx::query_scalar::<_,Uuid>("INSERT INTO receipts(courier_id,receipt_number,receipt_date,payment_method) VALUES($1,$2,$3,$4) RETURNING id")
 .bind(user.id).bind(input.receipt_number.trim()).bind(input.receipt_date).bind(input.payment_method).fetch_one(&mut *tx).await.map_err(|_|bad("Receipt number already exists for this date"))?;
 sqlx::query("INSERT INTO receipt_events(receipt_id,actor_id,action) VALUES($1,$2,'created')").bind(id).bind(user.id).execute(&mut *tx).await.map_err(|_|internal())?;
 tx.commit().await.map_err(|_|internal())?;Ok(Json(json!({"id":id,"status":"pending"})))
}
async fn receipts(State(app):State<Arc<App>>,headers:HeaderMap)->ApiResult<Value>{
 let user=actor(&app.db,&headers).await?;
 let rows=sqlx::query("SELECT r.id,r.receipt_number,r.receipt_date,r.payment_method,r.status,r.courier_id,c.full_name courier_name,a.full_name approver_name,x.full_name canceller_name,r.approved_at,r.cancelled_at,r.cancellation_reason FROM receipts r JOIN users c ON c.id=r.courier_id LEFT JOIN users a ON a.id=r.approved_by LEFT JOIN users x ON x.id=r.cancelled_by WHERE ($1='manager' OR r.courier_id=$2) ORDER BY r.created_at DESC LIMIT 2000")
 .bind(&user.role).bind(user.id).fetch_all(&app.db).await.map_err(|_|internal())?;
 Ok(Json(Value::Array(rows.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"receipt_number":r.get::<String,_>("receipt_number"),"receipt_date":r.get::<chrono::NaiveDate,_>("receipt_date"),"payment_method":r.get::<String,_>("payment_method"),"status":r.get::<String,_>("status"),"courier_id":r.get::<Uuid,_>("courier_id"),"courier_name":r.get::<String,_>("courier_name"),"approved_by":r.get::<Option<String>,_>("approver_name"),"cancelled_by":r.get::<Option<String>,_>("canceller_name"),"approved_at":r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("approved_at"),"cancelled_at":r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("cancelled_at"),"cancellation_reason":r.get::<Option<String>,_>("cancellation_reason")})).collect())))
}
async fn transition(app:Arc<App>,headers:HeaderMap,id:Uuid,action:&str,reason:Option<String>)->ApiResult<Value>{
 let user=actor(&app.db,&headers).await?;manager(&user)?;
 if action=="cancelled" && reason.as_deref().unwrap_or("").trim().is_empty(){return Err(bad("Cancellation reason required"))}
 let mut tx=app.db.begin().await.map_err(|_|internal())?;
 let result=match action {
 "approved"=>sqlx::query("UPDATE receipts SET status='approved',approved_by=$2,approved_at=now(),cancelled_by=NULL,cancelled_at=NULL,cancellation_reason=NULL WHERE id=$1 AND status='pending'").bind(id).bind(user.id).execute(&mut *tx).await,
 "cancelled"=>sqlx::query("UPDATE receipts SET status='cancelled',cancelled_by=$2,cancelled_at=now(),cancellation_reason=$3 WHERE id=$1 AND status='approved'").bind(id).bind(user.id).bind(&reason).execute(&mut *tx).await,
 "reactivated"=>sqlx::query("UPDATE receipts SET status='pending',approved_by=NULL,approved_at=NULL,cancelled_by=NULL,cancelled_at=NULL,cancellation_reason=NULL WHERE id=$1 AND status='cancelled'").bind(id).execute(&mut *tx).await,
 _=>return Err(bad("Unknown action"))
 }.map_err(|_|internal())?;
 if result.rows_affected()!=1{return Err(err(StatusCode::CONFLICT,"Receipt not found or invalid status transition"))}
 sqlx::query("INSERT INTO receipt_events(receipt_id,actor_id,action,reason) VALUES($1,$2,$3,$4)").bind(id).bind(user.id).bind(action).bind(reason).execute(&mut *tx).await.map_err(|_|internal())?;
 tx.commit().await.map_err(|_|internal())?;Ok(Json(json!({"id":id,"status":action})))
}
async fn approve(State(app):State<Arc<App>>,headers:HeaderMap,Path(id):Path<Uuid>)->ApiResult<Value>{transition(app,headers,id,"approved",None).await}
async fn cancel(State(app):State<Arc<App>>,headers:HeaderMap,Path(id):Path<Uuid>,Json(body):Json<Cancel>)->ApiResult<Value>{transition(app,headers,id,"cancelled",Some(body.reason)).await}
async fn reactivate(State(app):State<Arc<App>>,headers:HeaderMap,Path(id):Path<Uuid>)->ApiResult<Value>{transition(app,headers,id,"reactivated",None).await}
async fn history(State(app):State<Arc<App>>,headers:HeaderMap,Path(id):Path<Uuid>)->ApiResult<Value>{
 let user=actor(&app.db,&headers).await?;
 let allowed=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM receipts WHERE id=$1 AND ($2='manager' OR courier_id=$3))").bind(id).bind(&user.role).bind(user.id).fetch_one(&app.db).await.map_err(|_|internal())?;
 if !allowed{return Err(err(StatusCode::NOT_FOUND,"Receipt not found"))}
 let rows=sqlx::query("SELECT e.action,e.reason,e.created_at,u.full_name FROM receipt_events e JOIN users u ON u.id=e.actor_id WHERE e.receipt_id=$1 ORDER BY e.created_at,e.id").bind(id).fetch_all(&app.db).await.map_err(|_|internal())?;
 Ok(Json(Value::Array(rows.iter().map(|r|json!({"action":r.get::<String,_>("action"),"reason":r.get::<Option<String>,_>("reason"),"at":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"actor":r.get::<String,_>("full_name")})).collect())))
}
#[tokio::main] async fn main(){
 tracing_subscriber::fmt::init();
 let url=env::var("DATABASE_URL").expect("DATABASE_URL required");
 let db=PgPool::connect(&url).await.expect("Postgres connection failed");
 sqlx::migrate!().run(&db).await.expect("Migrations failed");
 let app=Router::new().route("/health",get(health)).route("/api/login",post(login)).route("/api/me",get(me))
 .route("/api/users",get(users).post(create_user)).route("/api/receipts",get(receipts).post(create_receipt))
 .route("/api/receipts/{id}/approve",post(approve)).route("/api/receipts/{id}/cancel",post(cancel))
 .route("/api/receipts/{id}/reactivate",post(reactivate)).route("/api/receipts/{id}/history",get(history))
 .with_state(Arc::new(App{db}));
 let port=env::var("PORT").unwrap_or_else(|_|"3000".into());
 let listener=tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await.expect("bind failed");
 axum::serve(listener,app).await.expect("server failed");
}
