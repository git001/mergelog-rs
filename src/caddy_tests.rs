use super::*;

const EXAMPLE: &str = r#"{"level":"info","ts":"2026-03-22T14:50:51.525Z","logger":"http.log.access","msg":"handled request","request":{"remote_ip":"10.128.7.169","remote_port":"59604","client_ip":"10.128.7.169","proto":"HTTP/1.1","method":"GET","host":"example.at","uri":"/foo/bar","headers":{"User-Agent":["GuzzleHttp/7"]}},"bytes_read":0,"user_id":"","duration":5739,"size":29558,"status":404,"resp_headers":{}}"#;

#[test]
fn parses_timestamp() {
    let (ts, _) = parse_caddy_line(EXAMPLE).expect("should parse");
    // 2026-03-22T14:50:51Z
    let expected = days_since_epoch(2026, 3, 22) * 86_400 + 14 * 3600 + 50 * 60 + 51;
    assert_eq!(ts, expected);
}

#[test]
fn produces_clf_line() {
    let (_, clf) = parse_caddy_line(EXAMPLE).expect("should parse");
    assert!(clf.starts_with("10.128.7.169 - - [22/Mar/2026:14:50:51 +0000]"));
    assert!(clf.contains(r#""GET /foo/bar HTTP/1.1""#));
    assert!(clf.contains("404 29558"));
    assert!(clf.contains(r#""-" "GuzzleHttp/7""#));
}

#[test]
fn float_ts() {
    let line = r#"{"ts":1742651451.525,"user_id":"","status":200,"size":100,"request":{"client_ip":"1.2.3.4","method":"GET","uri":"/","proto":"HTTP/1.1","headers":{}}}"#;
    let (ts, _) = parse_caddy_line(line).expect("should parse float ts");
    assert_eq!(ts, 1742651451_i64);
}

#[test]
fn non_empty_user_id() {
    let line = r#"{"ts":"2026-01-01T00:00:00Z","user_id":"alice","status":200,"size":42,"request":{"client_ip":"1.2.3.4","method":"GET","uri":"/","proto":"HTTP/1.1","headers":{}}}"#;
    let (_, clf) = parse_caddy_line(line).unwrap();
    assert!(clf.contains("- alice ["));
}

#[test]
fn zero_size_outputs_dash() {
    let line = r#"{"ts":"2026-01-01T00:00:00Z","user_id":"","status":304,"size":0,"request":{"client_ip":"1.2.3.4","method":"GET","uri":"/","proto":"HTTP/1.1","headers":{}}}"#;
    let (_, clf) = parse_caddy_line(line).unwrap();
    assert!(clf.contains("304 -"));
}

#[test]
fn missing_request_returns_none() {
    let line = r#"{"level":"error","ts":"2026-01-01T00:00:00Z","msg":"oops"}"#;
    assert!(parse_caddy_line(line).is_none());
}

#[test]
fn referer_and_ua_present() {
    let line = r#"{"ts":"2026-01-01T00:00:00Z","user_id":"","status":200,"size":1,"request":{"client_ip":"1.2.3.4","method":"GET","uri":"/","proto":"HTTP/1.1","headers":{"Referer":["https://example.com/"],"User-Agent":["TestBot/1.0"]}}}"#;
    let (_, clf) = parse_caddy_line(line).unwrap();
    assert!(clf.contains(r#""https://example.com/""#));
    assert!(clf.contains(r#""TestBot/1.0""#));
}
