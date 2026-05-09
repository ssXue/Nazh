#![allow(clippy::unwrap_used)]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use nazh_core::EngineError;
use serde_json::json;
use tokio::sync::Barrier;

use super::*;

fn http_connection(id: &str, url: &str) -> ConnectionDefinition {
    ConnectionDefinition {
        id: id.to_owned(),
        kind: "http".to_owned(),
        metadata: json!({
            "url": url,
            "method": "GET",
        }),
    }
}

fn http_connection_with_failure_threshold(id: &str, threshold: u32) -> ConnectionDefinition {
    ConnectionDefinition {
        id: id.to_owned(),
        kind: "http".to_owned(),
        metadata: json!({
            "url": "http://127.0.0.1/health",
            "method": "GET",
            "governance": {
                "circuit_failure_threshold": threshold,
                "circuit_open_ms": 1_000,
            },
        }),
    }
}

#[tokio::test]
async fn register_connection_拒绝未知连接类型并提示支持列表() {
    let manager = ConnectionManager::default();
    let err = manager
        .register_connection(ConnectionDefinition {
            id: "typo".to_owned(),
            kind: "modbux".to_owned(),
            metadata: json!({}),
        })
        .await
        .unwrap_err();

    let EngineError::ConnectionInvalidConfiguration {
        connection_id,
        reason,
    } = err
    else {
        panic!("未知连接类型应作为连接配置错误返回");
    };

    assert_eq!(connection_id, "typo");
    assert!(reason.contains("不支持的连接类型"));
    assert!(reason.contains("modbus"));
    assert!(reason.contains("ethercat"));
}

#[tokio::test]
async fn register_connection_拒绝缺少显式总线参数的_can_与_ethercat() {
    let manager = ConnectionManager::default();

    let can_err = manager
        .register_connection(ConnectionDefinition {
            id: "can-main".to_owned(),
            kind: "can".to_owned(),
            metadata: json!({
                "interface": "mock",
                "channel": "mock-can",
                "baud_rate": 115_200,
            }),
        })
        .await
        .unwrap_err();
    assert!(can_err.to_string().contains("bitrate"));

    let ethercat_err = manager
        .register_connection(ConnectionDefinition {
            id: "ecat-main".to_owned(),
            kind: "ethercat".to_owned(),
            metadata: json!({
                "interface": "en0",
                "cycle_time_ms": 5,
                "op_timeout_ms": 15_000,
            }),
        })
        .await
        .unwrap_err();
    assert!(ethercat_err.to_string().contains("backend"));
}

#[tokio::test]
async fn ensure_shared_session_同一连接并发只初始化一次() {
    let manager = Arc::new(ConnectionManager::default());
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(8));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let manager = Arc::clone(&manager);
        let factory_calls = Arc::clone(&factory_calls);
        let barrier = Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            manager
                .ensure_shared_session("shared-can", || async move {
                    factory_calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Ok::<usize, EngineError>(42)
                })
                .await
                .unwrap()
        }));
    }

    let mut sessions = Vec::new();
    for handle in handles {
        sessions.push(handle.await.unwrap());
    }

    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
    for session in &sessions[1..] {
        assert!(Arc::ptr_eq(&sessions[0], session));
    }
}

#[tokio::test]
async fn upsert_connection_不替换正在借出的连接记录() {
    let manager = ConnectionManager::default();
    manager
        .register_connection(http_connection("http-main", "http://127.0.0.1/old"))
        .await
        .unwrap();
    let guard = manager.acquire("http-main").await.unwrap();

    manager
        .upsert_connection(http_connection("http-main", "http://127.0.0.1/new"))
        .await;

    let Err(err) = manager.acquire("http-main").await else {
        panic!("正在借出的连接不应被新记录替换为可再次借出");
    };
    assert!(matches!(err, EngineError::ConnectionBusy(id) if id == "http-main"));

    drop(guard);
    let record = manager.get("http-main").await.unwrap();
    assert_eq!(record.metadata["url"], "http://127.0.0.1/old");
}

#[tokio::test]
async fn replace_connections_不替换正在借出的连接记录() {
    let manager = ConnectionManager::default();
    manager
        .register_connection(http_connection("http-main", "http://127.0.0.1/old"))
        .await
        .unwrap();
    let guard = manager.acquire("http-main").await.unwrap();

    manager
        .replace_connections([http_connection("http-main", "http://127.0.0.1/new")])
        .await;

    let Err(err) = manager.acquire("http-main").await else {
        panic!("整体替换不应绕过正在借出的旧记录");
    };
    assert!(matches!(err, EngineError::ConnectionBusy(id) if id == "http-main"));

    drop(guard);
    let record = manager.get("http-main").await.unwrap();
    assert_eq!(record.metadata["url"], "http://127.0.0.1/old");
}

#[tokio::test]
async fn mark_failure_推进失败计数并进入熔断() {
    let manager = ConnectionManager::default();
    manager
        .register_connection(http_connection_with_failure_threshold("http-main", 2))
        .await
        .unwrap();

    for expected_failures in 1..=2 {
        let mut guard = manager.acquire("http-main").await.unwrap();
        guard.mark_failure("连接被对端拒绝");
        drop(guard);

        let record = manager.get("http-main").await.unwrap();
        assert_eq!(record.health.total_failures, expected_failures);
        assert_eq!(record.health.consecutive_failures, expected_failures);
    }

    let record = manager.get("http-main").await.unwrap();
    assert_eq!(record.health.phase, ConnectionHealthState::CircuitOpen);
    assert!(record.health.circuit_open_until.is_some());

    let Err(err) = manager.acquire("http-main").await else {
        panic!("达到失败阈值后应进入熔断并拒绝继续借出");
    };
    assert!(matches!(
        err,
        EngineError::ConnectionCircuitOpen {
            connection_id,
            ..
        } if connection_id == "http-main"
    ));
}

#[tokio::test]
async fn mark_failure_已手动记录连接失败时不重复计数() {
    let manager = ConnectionManager::default();
    manager
        .register_connection(http_connection_with_failure_threshold("http-main", 3))
        .await
        .unwrap();

    let mut guard = manager.acquire("http-main").await.unwrap();
    let _retry_after_ms = manager
        .record_connect_failure("http-main", "连接被对端拒绝")
        .await
        .unwrap();
    guard.mark_failure("连接被对端拒绝");
    drop(guard);

    let record = manager.get("http-main").await.unwrap();
    assert_eq!(record.health.total_failures, 1);
    assert_eq!(record.health.consecutive_failures, 1);
    assert_eq!(record.health.phase, ConnectionHealthState::Reconnecting);
}
