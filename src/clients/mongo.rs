use mongodb::{Client, Database, options::ClientOptions};

pub async fn connect(uri: &str, db_name: &str) -> mongodb::error::Result<Database> {
    let mut options = ClientOptions::parse(uri).await?;
    options.app_name = Some("jana2u-pos-backend".to_string());

    let client = Client::with_options(options)?;

    // Fail fast on a bad connection string / unreachable server rather than
    // discovering it on the first request.
    client
        .database(db_name)
        .run_command(mongodb::bson::doc! { "ping": 1 })
        .await?;

    Ok(client.database(db_name))
}
