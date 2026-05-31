use prober_core::model::ProbeKind;
use prober_core::probes::ProbeFn;

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

#[test]
fn probe_kinds_serialize_to_snake_case() {
    let cases = [
        (ProbeKind::DnsResolve, "dns_resolve"),
        (ProbeKind::HttpsJsonRpcWrite, "https_json_rpc_write"),
        (ProbeKind::P2pTcpConnect, "p2p_tcp_connect"),
        (ProbeKind::LibP2pHandshake, "lib_p2p_handshake"),
    ];
    for (kind, expected) in cases {
        assert_eq!(
            serde_json::to_string(&kind).unwrap(),
            format!("\"{expected}\"")
        );
    }
}
