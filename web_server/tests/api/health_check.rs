use reqwest::Response;

use crate::helpers::spawn_app;

#[tokio::test]
async fn health_check_test() {
    let app = spawn_app().await;

    let response = check_endpoint("/management/health", &app).await;
    let status_code = response.status();
    let response_body = response
        .text()
        .await
        .expect("Cannot read health check response body");

    assert!(status_code.is_success());
    assert_eq!("Web server is up", response_body);
}

async fn check_endpoint(endpoint: &str, app: &crate::helpers::TestApp) -> Response {
    let client = reqwest::Client::new();

    client
        .get(format!("{}{}", &app.address, endpoint))
        .send()
        .await
        .unwrap_or_else(|e| panic!("Failed to request path={endpoint}. error={e}"))
}
