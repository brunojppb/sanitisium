use std::fs;
use tempfile::NamedTempFile;

use crate::helpers::spawn_app;

#[tokio::test]
async fn enqueue_pdf_success() {
    let app = spawn_app().await;

    // Create a temporary PDF file for testing
    let test_pdf_content = create_minimal_pdf_content();
    let temp_pdf = NamedTempFile::new().expect("Failed to create temporary PDF file");
    fs::write(temp_pdf.path(), test_pdf_content).expect("Failed to write test PDF content");

    // Read the test PDF content
    let pdf_bytes = fs::read(temp_pdf.path()).expect("Failed to read test PDF file");

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/sanitise/pdf", &app.address))
        .query(&[
            ("id", "test-123"),
            ("success_callback_url", "http://example.com/success"),
            ("failure_callback_url", "http://example.com/failure"),
        ])
        .header("Content-Type", "application/pdf")
        .body(pdf_bytes)
        .send()
        .await
        .expect("Failed to execute request");

    let status_code = response.status();
    let response_body = response.text().await.expect("Failed to read response body");

    assert!(status_code.is_success());
    assert_eq!("PDF added to queue for processing", response_body);
}

#[tokio::test]
async fn enqueue_pdf_missing_query_params() {
    let app = spawn_app().await;

    // Create a temporary PDF file for testing
    let test_pdf_content = create_minimal_pdf_content();
    let temp_pdf = NamedTempFile::new().expect("Failed to create temporary PDF file");
    fs::write(temp_pdf.path(), test_pdf_content).expect("Failed to write test PDF content");

    // Read the test PDF content
    let pdf_bytes = fs::read(temp_pdf.path()).expect("Failed to read test PDF file");

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/sanitise/pdf", &app.address))
        // Missing required query parameters
        .header("Content-Type", "application/pdf")
        .body(pdf_bytes)
        .send()
        .await
        .expect("Failed to execute request");

    let status_code = response.status();

    // Should fail due to missing query parameters
    assert!(!status_code.is_success());
    assert_eq!(status_code, 400);
}

#[tokio::test]
async fn enqueue_pdf_with_test_pdf_file() {
    let app = spawn_app().await;

    let test_pdf_content = create_minimal_pdf_content();
    let temp_pdf = NamedTempFile::new().expect("Failed to create temporary PDF file");
    fs::write(temp_pdf.path(), test_pdf_content).expect("Failed to write test PDF content");

    let pdf_bytes = fs::read(temp_pdf.path()).expect("Failed to read test PDF file");

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/sanitise/pdf", &app.address))
        .query(&[
            ("id", "test-real-pdf"),
            ("success_callback_url", "http://example.com/success"),
            ("failure_callback_url", "http://example.com/failure"),
        ])
        .header("Content-Type", "application/pdf")
        .body(pdf_bytes)
        .send()
        .await
        .expect("Failed to execute request");

    let status_code = response.status();
    let response_body = response.text().await.expect("Failed to read response body");

    assert!(status_code.is_success());
    assert_eq!("PDF added to queue for processing", response_body);
}

/// Creates minimal PDF content for testing purposes
/// This is a simple valid PDF that can be used for testing file uploads
fn create_minimal_pdf_content() -> Vec<u8> {
    // This is a minimal valid PDF structure
    let pdf_content = b"%PDF-1.4
1 0 obj
<<
/Type /Catalog
/Pages 2 0 R
>>
endobj

2 0 obj
<<
/Type /Pages
/Kids [3 0 R]
/Count 1
>>
endobj

3 0 obj
<<
/Type /Page
/Parent 2 0 R
/MediaBox [0 0 612 792]
>>
endobj

xref
0 4
0000000000 65535 f 
0000000010 00000 n 
0000000053 00000 n 
0000000101 00000 n 
trailer
<<
/Size 4
/Root 1 0 R
>>
startxref
157
%%EOF";

    pdf_content.to_vec()
}
