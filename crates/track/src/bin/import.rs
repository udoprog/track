use anyhow::Result;

#[tokio::main]
pub async fn main() -> Result<()> {
    track::import().await
}
