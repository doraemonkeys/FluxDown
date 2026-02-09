mod download_actor;

use std::env;
use std::path::PathBuf;

pub async fn create_actors() {
    let db_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    download_actor::run(db_dir).await;
}
