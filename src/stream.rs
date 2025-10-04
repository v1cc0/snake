use axum::{
    body::Body,
    http::{StatusCode, header},
    response::Response,
};
use serde_json::{Value, json};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info, error};

/// Converts a complete response to SSE (Server-Sent Events) stream format
pub fn convert_to_sse_stream(status: StatusCode, response_bytes: bytes::Bytes) -> Response {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, std::io::Error>>(100);

    tokio::spawn(async move {
        // Parse the response JSON
        if let Ok(json_response) = serde_json::from_slice::<Value>(&response_bytes) {
            info!("SSE: Successfully parsed JSON response");
            // Check if it's a chat completion response
            if let Some(choices) = json_response.get("choices").and_then(|v| v.as_array()) {
                info!("SSE: Found choices array with {} items", choices.len());
                if let Some(first_choice) = choices.first() {
                    info!("SSE: Processing first choice");
                    // Extract the full content from the message
                    if let Some(content) = first_choice
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        info!("SSE: Extracted content, length: {} chars", content.len());
                        // Get metadata
                        let created = json_response.get("created").cloned().unwrap_or(json!(0));
                        let id = json_response.get("id").cloned().unwrap_or(json!("unknown"));
                        let model = json_response
                            .get("model")
                            .cloned()
                            .unwrap_or(json!("unknown"));

                        // Split content into words for streaming simulation
                        let words: Vec<&str> = content.split_whitespace().collect();
                        info!("SSE: Split content into {} words", words.len());

                        // Send chunks with delays to simulate streaming
                        for (i, word) in words.iter().enumerate() {
                            let word_with_space = if i < words.len() - 1 {
                                format!("{} ", word)
                            } else {
                                word.to_string()
                            };

                            let chunk = json!({
                                "id": id,
                                "object": "chat.completion.chunk",
                                "created": created,
                                "model": model,
                                "choices": [{
                                    "index": 0,
                                    "delta": {
                                        "content": word_with_space
                                    },
                                    "finish_reason": null
                                }]
                            });

                            let sse_data = format!(
                                "data: {}\n\n",
                                serde_json::to_string(&chunk).unwrap_or_default()
                            );
                            if tx.send(Ok(sse_data)).await.is_err() {
                                return;
                            }

                            // Add small delay between chunks
                            tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
                        }

                        // Send final chunk with finish_reason and usage
                        let mut final_chunk = json!({
                            "id": id,
                            "object": "chat.completion.chunk",
                            "created": created,
                            "model": model,
                            "choices": [{
                                "index": 0,
                                "delta": {},
                                "finish_reason": first_choice.get("finish_reason").cloned().unwrap_or(json!("stop"))
                            }]
                        });

                        // Add usage info if available
                        if let Some(usage) = json_response.get("usage") {
                            final_chunk["usage"] = usage.clone();
                        }

                        let sse_data = format!(
                            "data: {}\n\n",
                            serde_json::to_string(&final_chunk).unwrap_or_default()
                        );
                        let _ = tx.send(Ok(sse_data)).await;
                        info!("SSE: Sent final chunk with finish_reason");
                    } else {
                        error!("SSE: No content found in message field");
                        // No content found, send the choice as-is
                        let chunk = json!({
                            "choices": [first_choice],
                            "created": json_response.get("created").cloned().unwrap_or(json!(0)),
                            "id": json_response.get("id").cloned().unwrap_or(json!("unknown")),
                            "model": json_response.get("model").cloned().unwrap_or(json!("unknown")),
                            "object": "chat.completion.chunk"
                        });
                        let sse_data = format!(
                            "data: {}\n\n",
                            serde_json::to_string(&chunk).unwrap_or_default()
                        );
                        let _ = tx.send(Ok(sse_data)).await;
                    }
                } else {
                    error!("SSE: Choices array is empty");
                }
            } else {
                error!("SSE: No choices field found in response");
                // Not a standard chat completion, send the whole response as one chunk
                let sse_data = format!(
                    "data: {}\n\n",
                    serde_json::to_string(&json_response).unwrap_or_default()
                );
                let _ = tx.send(Ok(sse_data)).await;
            }
        } else {
            error!("SSE: Failed to parse response as JSON");
            // Failed to parse JSON, send raw data
            if let Ok(text) = String::from_utf8(response_bytes.to_vec()) {
                error!("SSE: Response text preview: {}", &text[..text.len().min(200)]);
                let sse_data = format!("data: {}\n\n", text);
                let _ = tx.send(Ok(sse_data)).await;
            }
        }

        // Send the [DONE] marker
        info!("SSE: Sending [DONE] marker");
        let _ = tx.send(Ok("data: [DONE]\n\n".to_string())).await;
    });

    let stream = ReceiverStream::new(rx);
    let body = Body::from_stream(stream);

    let mut response = Response::new(body);
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, "text/event-stream".parse().unwrap());
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    response
        .headers_mut()
        .insert(header::CONNECTION, "keep-alive".parse().unwrap());

    response
}
