use actix_web::{test, web, App};
use music_browser::app::{trust_file, trust_page, TRUST_DIR_ENV};

// One test so the process-wide TRUST_DIR isn't raced.
#[actix_web::test]
async fn trust_endpoints_serve_only_public_ca_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("ca.crt"), "PEM").unwrap();
    std::fs::write(dir.path().join("ca.key"), "SECRET").unwrap();
    std::fs::write(dir.path().join("fingerprint"), "SHA-256 AA:BB<script>\n").unwrap();
    std::env::set_var(TRUST_DIR_ENV, dir.path());

    let app = test::init_service(
        App::new()
            .route("/trust", web::get().to(trust_page))
            .route("/trust/{file}", web::get().to(trust_file)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/trust")
        .insert_header(("Host", "music.home.arpa:80"))
        .to_request();
    let body = String::from_utf8(test::call_and_read_body(&app, req).await.to_vec()).unwrap();
    assert!(body.contains("SHA-256 AA:BBscript"));
    assert!(!body.contains("<script>"));
    assert!(body.contains("https://music.home.arpa/"));

    let req = test::TestRequest::get().uri("/trust/ca.crt").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(test::read_body(resp).await, "PEM");

    for path in ["/trust/ca.key", "/trust/..%2Fca.key", "/trust/ca.cer"] {
        let req = test::TestRequest::get().uri(path).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404, "{path}");
    }

    std::env::remove_var(TRUST_DIR_ENV);
    let req = test::TestRequest::get().uri("/trust").to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 404);
}
