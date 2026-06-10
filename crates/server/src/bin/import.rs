use anyhow::Result;

#[tokio::main]
pub async fn main() -> Result<()> {
    server::import().await
}
