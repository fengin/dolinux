use serde::{Deserialize, Serialize};
use serde_json::Value;

/// CDP JSON-RPC 请求
#[derive(Debug, Serialize)]
pub struct CdpRequest {
    pub id: u32,
    pub method: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

/// CDP JSON-RPC 响应
#[derive(Debug, Deserialize)]
pub struct CdpResponse {
    pub id: Option<u32>,
    pub result: Option<Value>,
    pub error: Option<CdpError>,
    pub method: Option<String>,
    pub params: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct CdpError {
    pub code: i64,
    pub message: String,
}

/// Chrome /json/list 返回的页面信息
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub id: String,
    pub title: String,
    pub url: String,
    #[serde(rename = "type")]
    pub page_type: String,
    pub web_socket_debugger_url: Option<String>,
}
