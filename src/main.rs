#[tokio::main]
async fn main() -> anyhow::Result<()> {
    gitwatch_v2::run().await
}
