//! End-to-end tests of the Open API client against a local mock server.

use lgpui::api::{ApiError, LarkClient};
use lgpui::config::Config;
use mockito::{Matcher, Server};

fn client(server: &Server) -> LarkClient {
    LarkClient::new(Config {
        app_id: "cli_test".into(),
        app_secret: "secret".into(),
        base_url: server.url(),
    })
}

fn mock_token(server: &mut Server) -> mockito::Mock {
    server
        .mock("POST", "/open-apis/auth/v3/tenant_access_token/internal")
        .match_body(Matcher::PartialJsonString(r#"{"app_id":"cli_test","app_secret":"secret"}"#.into()))
        .with_header("content-type", "application/json")
        .with_body(r#"{"code":0,"msg":"ok","tenant_access_token":"t-abc","expire":7200}"#)
        .expect(1)
        .create()
}

#[test]
fn token_is_cached_and_chats_paginate() {
    let mut server = Server::new();
    let token = mock_token(&mut server);
    let p1 = server
        .mock("GET", "/open-apis/im/v1/chats")
        .match_header("authorization", "Bearer t-abc")
        .match_query(Matcher::Regex("^page_size=50$".into()))
        .with_body(r#"{"code":0,"msg":"ok","data":{"items":[{"chat_id":"oc_1","name":"One"}],"has_more":true,"page_token":"p2"}}"#)
        .create();
    let p2 = server
        .mock("GET", "/open-apis/im/v1/chats")
        .match_query(Matcher::UrlEncoded("page_token".into(), "p2".into()))
        .with_body(r#"{"code":0,"msg":"ok","data":{"items":[{"chat_id":"oc_2","name":"Two"}],"has_more":false,"page_token":""}}"#)
        .create();

    let c = client(&server);
    let chats = c.list_chats(10).unwrap();
    assert_eq!(chats.iter().map(|c| c.chat_id.as_str()).collect::<Vec<_>>(), ["oc_1", "oc_2"]);
    token.assert();
    p1.assert();
    p2.assert();
}

#[test]
fn messages_are_returned_oldest_first() {
    let mut server = Server::new();
    mock_token(&mut server);
    let m = server
        .mock("GET", "/open-apis/im/v1/messages")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("container_id_type".into(), "chat".into()),
            Matcher::UrlEncoded("container_id".into(), "oc_1".into()),
            Matcher::UrlEncoded("sort_type".into(), "ByCreateTimeDesc".into()),
        ]))
        .with_body(r#"{"code":0,"data":{"items":[
            {"message_id":"om_2","msg_type":"text","create_time":"2000","chat_id":"oc_1","body":{"content":"{\"text\":\"second\"}"}},
            {"message_id":"om_1","msg_type":"text","create_time":"1000","chat_id":"oc_1","body":{"content":"{\"text\":\"first\"}"}}
        ],"has_more":false}}"#)
        .create();
    let msgs = client(&server).list_messages("oc_1", 30).unwrap();
    m.assert();
    assert_eq!(msgs[0].plain_text(), "first");
    assert_eq!(msgs[1].plain_text(), "second");
}

#[test]
fn send_text_posts_json_content() {
    let mut server = Server::new();
    mock_token(&mut server);
    let m = server
        .mock("POST", "/open-apis/im/v1/messages")
        .match_query(Matcher::UrlEncoded("receive_id_type".into(), "chat_id".into()))
        .match_body(Matcher::Json(serde_json::json!({
            "receive_id": "oc_1",
            "msg_type": "text",
            "content": "{\"text\":\"hi \\\"you\\\"\"}"
        })))
        .with_body(r#"{"code":0,"data":{"message_id":"om_9","msg_type":"text","create_time":"3000","chat_id":"oc_1","body":{"content":"{\"text\":\"hi \\\"you\\\"\"}"}}}"#)
        .create();
    let sent = client(&server).send_text("oc_1", "hi \"you\"").unwrap();
    m.assert();
    assert_eq!(sent.message_id, "om_9");
    assert_eq!(sent.plain_text(), "hi \"you\"");
}

#[test]
fn lark_errors_are_surfaced() {
    let mut server = Server::new();
    server
        .mock("POST", "/open-apis/auth/v3/tenant_access_token/internal")
        .with_status(400)
        .with_body(r#"{"code":10003,"msg":"invalid param"}"#)
        .create();
    match client(&server).list_chats(1) {
        Err(ApiError::Lark { code, msg }) => {
            assert_eq!(code, 10003);
            assert_eq!(msg, "invalid param");
        }
        other => panic!("expected Lark error, got {other:?}"),
    }
}

#[test]
fn api_error_code_in_envelope() {
    let mut server = Server::new();
    mock_token(&mut server);
    server
        .mock("GET", "/open-apis/im/v1/messages")
        .match_query(Matcher::Any)
        .with_body(r#"{"code":230002,"msg":"Bot/User can NOT be out of the chat."}"#)
        .create();
    let err = client(&server).list_messages("oc_x", 10).unwrap_err();
    assert!(err.to_string().contains("230002"), "{err}");
}
