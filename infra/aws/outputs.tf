output "sync_endpoint" {
  description = "Set this as sync.endpoint in Pulse config.toml."
  value       = "${aws_apigatewayv2_api.sync.api_endpoint}/v1/pulse/sync"
}

output "search_endpoint" {
  description = "Authenticated semantic search endpoint."
  value       = "${aws_apigatewayv2_api.sync.api_endpoint}/v1/pulse/search"
}

output "artifact_upload_endpoint" {
  description = "Authenticated endpoint for approved S3 artifact upload URLs."
  value       = "${aws_apigatewayv2_api.sync.api_endpoint}/v1/pulse/artifacts/upload-url"
}

output "archive_bucket" {
  value = aws_s3_bucket.archive.id
}

output "sync_token" {
  description = "Store as PULSE_SYNC_TOKEN on the desktop; never place it in config.toml."
  value       = random_password.sync_token.result
  sensitive   = true
}
