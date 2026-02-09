use prober_core::model::ProbeKind;

#[tokio::test]
async fn http_control_works_with_mockito() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/ping")
        .with_status(200)
        .with_body("ok")
        .create_async()
        .await;

    let probe = prober_core::probes::http_control::HttpControlProbe {
        url: format!("{}/ping", server.url()),
        expect_body: Some("ok".to_string()),
    };

    let r = probe.run(2000).await;
    mock.assert_async().await;
    assert!(r.ok);
}

#[tokio::test]
async fn jsonrpc_works_with_mockito() {
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("POST", "/")
        .with_status(200)
        .with_body(r#"{"jsonrpc":"2.0","id":1,"result":"0x1"}"#)
        .create_async()
        .await;

    let probe = prober_core::probes::https_jsonrpc::HttpsJsonRpcProbe {
        url: server.url(),
        method: "eth_chainId".to_string(),
    };

    let r = probe.run(2000).await;
    mock.assert_async().await;
    assert!(r.ok);
}

#[tokio::test]
async fn model_kinds_are_stable() {
    let kinds: Array<ProbeKind> = [
        ProbeKind::DnsResolve,
        ProbeKind::TcpConnect,
        ProbeKind::HttpControl,
        ProbeKind::HttpsJsonRpc,
        ProbeKind::WssJsonRpc,
        ProbeKind::Discv5Ping,
    ];
    assert_eq!(kinds.len(), 6);
}

// Minimal TS-like alias for the test above while staying in Rust:
// (kept intentionally tiny)
type Array<T> = Vec<T>;
