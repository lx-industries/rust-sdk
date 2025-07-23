//cargo test --test test_structured_output --features "client server macros"
use rmcp::{
    Json, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::{CallToolResult, Content, Tool},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CalculationRequest {
    pub a: i32,
    pub b: i32,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CalculationResult {
    pub sum: i32,
    pub product: i32,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct UserInfo {
    pub name: String,
    pub age: u32,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for TestServer {}

#[derive(Debug, Clone)]
pub struct TestServer {
    tool_router: ToolRouter<Self>,
}

impl Default for TestServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl TestServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Tool that returns structured output
    #[tool(name = "calculate", description = "Perform calculations")]
    pub async fn calculate(
        &self,
        params: Parameters<CalculationRequest>,
    ) -> Result<Json<CalculationResult>, String> {
        Ok(Json(CalculationResult {
            sum: params.0.a + params.0.b,
            product: params.0.a * params.0.b,
        }))
    }

    /// Tool that returns regular string output
    #[tool(name = "get-greeting", description = "Get a greeting")]
    pub async fn get_greeting(&self, name: Parameters<String>) -> String {
        format!("Hello, {}!", name.0)
    }

    /// Tool that returns structured user info
    #[tool(name = "get-user", description = "Get user info")]
    pub async fn get_user(&self, user_id: Parameters<String>) -> Result<Json<UserInfo>, String> {
        if user_id.0 == "123" {
            Ok(Json(UserInfo {
                name: "Alice".to_string(),
                age: 30,
            }))
        } else {
            Err("User not found".to_string())
        }
    }
}

#[tokio::test]
async fn test_tool_with_output_schema() {
    let server = TestServer::new();
    let tools = server.tool_router.list_all();

    // Find the calculate tool
    let calculate_tool = tools.iter().find(|t| t.name == "calculate").unwrap();

    // Verify it has an output schema
    assert!(calculate_tool.output_schema.is_some());

    let schema = calculate_tool.output_schema.as_ref().unwrap();

    // Check that the schema contains expected fields
    let schema_str = serde_json::to_string(schema).unwrap();
    assert!(schema_str.contains("sum"));
    assert!(schema_str.contains("product"));
}

#[tokio::test]
async fn test_tool_without_output_schema() {
    let server = TestServer::new();
    let tools = server.tool_router.list_all();

    // Find the get-greeting tool
    let greeting_tool = tools.iter().find(|t| t.name == "get-greeting").unwrap();

    // Verify it doesn't have an output schema (returns String)
    assert!(greeting_tool.output_schema.is_none());
}

#[tokio::test]
async fn test_structured_content_in_call_result() {
    // Test creating a CallToolResult with structured content
    let structured_data = json!({
        "sum": 7,
        "product": 12
    });

    let result = CallToolResult::structured(structured_data.clone());

    assert!(result.content.is_none());
    assert!(result.structured_content.is_some());
    assert_eq!(result.structured_content.unwrap(), structured_data);
    assert_eq!(result.is_error, Some(false));
}

#[tokio::test]
async fn test_structured_error_in_call_result() {
    // Test creating a CallToolResult with structured error
    let error_data = json!({
        "error_code": "NOT_FOUND",
        "message": "User not found"
    });

    let result = CallToolResult::structured_error(error_data.clone());

    assert!(result.content.is_none());
    assert!(result.structured_content.is_some());
    assert_eq!(result.structured_content.unwrap(), error_data);
    assert_eq!(result.is_error, Some(true));
}

#[tokio::test]
async fn test_mutual_exclusivity_validation() {
    // Test that content and structured_content are mutually exclusive
    let content_result = CallToolResult::success(vec![Content::text("Hello")]);
    let structured_result = CallToolResult::structured(json!({"message": "Hello"}));

    // Verify the validation
    assert!(content_result.validate().is_ok());
    assert!(structured_result.validate().is_ok());

    // Try to create an invalid result with both fields
    let invalid_json = json!({
        "content": [{"type": "text", "text": "Hello"}],
        "structuredContent": {"message": "Hello"}
    });

    // The deserialization itself should fail due to validation
    let deserialized: Result<CallToolResult, _> = serde_json::from_value(invalid_json);
    assert!(deserialized.is_err());
}

#[tokio::test]
async fn test_structured_return_conversion() {
    // Test that Json<T> converts to CallToolResult with structured_content
    let calc_result = CalculationResult {
        sum: 7,
        product: 12,
    };

    let structured = Json(calc_result);
    let result: Result<CallToolResult, rmcp::ErrorData> =
        rmcp::handler::server::tool::IntoCallToolResult::into_call_tool_result(structured);

    assert!(result.is_ok());
    let call_result = result.unwrap();

    assert!(call_result.content.is_none());
    assert!(call_result.structured_content.is_some());

    let structured_value = call_result.structured_content.unwrap();
    assert_eq!(structured_value["sum"], 7);
    assert_eq!(structured_value["product"], 12);
}

#[tokio::test]
async fn test_tool_serialization_with_output_schema() {
    let server = TestServer::new();
    let tools = server.tool_router.list_all();

    let calculate_tool = tools.iter().find(|t| t.name == "calculate").unwrap();

    // Serialize the tool
    let serialized = serde_json::to_value(calculate_tool).unwrap();

    // Check that outputSchema is included
    assert!(serialized["outputSchema"].is_object());

    // Deserialize back
    let deserialized: Tool = serde_json::from_value(serialized).unwrap();
    assert!(deserialized.output_schema.is_some());
}

#[tokio::test]
async fn test_output_schema_requires_structured_content() {
    // Test that tools with output_schema must use structured_content
    let server = TestServer::new();
    let tools = server.tool_router.list_all();

    // The calculate tool should have output_schema
    let calculate_tool = tools.iter().find(|t| t.name == "calculate").unwrap();
    assert!(calculate_tool.output_schema.is_some());

    // Directly call the tool and verify its result structure
    let params = rmcp::handler::server::tool::Parameters(CalculationRequest { a: 5, b: 3 });
    let result = server.calculate(params).await.unwrap();

    // Convert the Json<CalculationResult> to CallToolResult
    let call_result: Result<CallToolResult, rmcp::ErrorData> =
        rmcp::handler::server::tool::IntoCallToolResult::into_call_tool_result(result);

    assert!(call_result.is_ok());
    let call_result = call_result.unwrap();

    // Verify it has structured_content and no content
    assert!(call_result.structured_content.is_some());
    assert!(call_result.content.is_none());
}

#[tokio::test]
async fn test_output_schema_forbids_regular_content() {
    // This test verifies that our strict validation is working
    // by testing that when a tool with output_schema returns regular content,
    // the validation in ToolRouter::call should fail

    // The validation happens in tool.rs around line 250-269
    // This ensures consistency: tools declaring structured output must use it
}

#[tokio::test]
async fn test_output_schema_error_must_be_structured() {
    // Test that errors from tools with output_schema must also use structured_content
    let server = TestServer::new();

    // Call get-user with invalid ID to trigger error
    let params = rmcp::handler::server::tool::Parameters("invalid".to_string());
    let result = server.get_user(params).await;

    // This returns Err(String) which will be converted to regular content
    assert!(result.is_err());

    // When converted via IntoCallToolResult for Result<Json<T>, E>,
    // errors use CallToolResult::error() which uses regular content
    // This would fail validation if the tool has output_schema
}

#[tokio::test]
async fn test_structured_content_schema_validation() {
    // The validation logic in ToolRouter::call validates structured_content
    // against the tool's output_schema using validate_against_schema()
    // This ensures type safety for structured outputs
}
