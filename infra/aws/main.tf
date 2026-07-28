provider "aws" {
  region = var.aws_region
}

locals {
  function_name = "${var.name_prefix}-sync-api"
  common_tags = {
    Application = "Pulse"
    ManagedBy   = "Terraform"
    Workstream  = "5"
  }
}

resource "random_password" "sync_token" {
  length  = 48
  special = false
}

resource "aws_secretsmanager_secret" "sync" {
  name                    = "${var.name_prefix}/sync-api"
  recovery_window_in_days = 7
  tags                    = local.common_tags
}

resource "aws_secretsmanager_secret_version" "sync" {
  secret_id = aws_secretsmanager_secret.sync.id
  secret_string = jsonencode({
    cockroach_connection_string = var.cockroach_connection_string
    sync_token                  = random_password.sync_token.result
  })
}

resource "aws_s3_bucket" "archive" {
  bucket_prefix = "${var.name_prefix}-archive-"
  force_destroy = false
  tags          = local.common_tags
}

resource "aws_s3_bucket_public_access_block" "archive" {
  bucket                  = aws_s3_bucket.archive.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_server_side_encryption_configuration" "archive" {
  bucket = aws_s3_bucket.archive.id
  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_versioning" "archive" {
  bucket = aws_s3_bucket.archive.id
  versioning_configuration { status = "Enabled" }
}

resource "aws_s3_bucket_lifecycle_configuration" "archive" {
  bucket = aws_s3_bucket.archive.id
  rule {
    id     = "archive-retention"
    status = "Enabled"
    filter {}
    expiration { days = var.archive_retention_days }
    noncurrent_version_expiration { noncurrent_days = 30 }
  }
}

resource "aws_iam_role" "lambda" {
  name = "${local.function_name}-role"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "lambda.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })
  tags = local.common_tags
}

resource "aws_iam_role_policy" "lambda" {
  name = "${local.function_name}-least-privilege"
  role = aws_iam_role.lambda.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = ["logs:CreateLogStream", "logs:PutLogEvents"]
        Resource = "${aws_cloudwatch_log_group.lambda.arn}:*"
      },
      {
        Effect   = "Allow"
        Action   = ["secretsmanager:GetSecretValue"]
        Resource = aws_secretsmanager_secret.sync.arn
      },
      {
        Effect   = "Allow"
        Action   = ["s3:GetObject", "s3:PutObject"]
        Resource = "${aws_s3_bucket.archive.arn}/*"
      }
    ]
  })
}

resource "aws_cloudwatch_log_group" "lambda" {
  name              = "/aws/lambda/${local.function_name}"
  retention_in_days = var.log_retention_days
  tags              = local.common_tags
}

# The archive data source is delayed until npm has installed Lambda's production
# dependencies. Run `terraform apply` from this directory; no global packager is
# required beyond Node/npm and Terraform.
resource "terraform_data" "lambda_dependencies" {
  triggers_replace = [
    filesha256("${path.module}/lambda/index.mjs"),
    filesha256("${path.module}/lambda/package.json"),
    filesha256("${path.module}/lambda/package-lock.json"),
  ]

  provisioner "local-exec" {
    command     = "npm ci --omit=dev --no-audit --no-fund"
    working_dir = "${path.module}/lambda"
  }
}

data "archive_file" "lambda" {
  type        = "zip"
  source_dir  = "${path.module}/lambda"
  output_path = "${path.module}/.build/${local.function_name}.zip"
  excludes    = [".build", "package-lock.json"]
  depends_on  = [terraform_data.lambda_dependencies]
}

resource "aws_lambda_function" "sync" {
  function_name    = local.function_name
  role             = aws_iam_role.lambda.arn
  handler          = "index.handler"
  runtime          = "nodejs22.x"
  filename         = data.archive_file.lambda.output_path
  source_code_hash = data.archive_file.lambda.output_base64sha256
  timeout          = 29
  memory_size      = 512

  environment {
    variables = {
      PULSE_SECRET_ARN     = aws_secretsmanager_secret.sync.arn
      PULSE_ARCHIVE_BUCKET = aws_s3_bucket.archive.id
    }
  }

  depends_on = [aws_cloudwatch_log_group.lambda, aws_iam_role_policy.lambda]
  tags       = local.common_tags
}

resource "aws_apigatewayv2_api" "sync" {
  name          = "${local.function_name}-http"
  protocol_type = "HTTP"
  tags          = local.common_tags
}

resource "aws_apigatewayv2_integration" "sync" {
  api_id                 = aws_apigatewayv2_api.sync.id
  integration_type       = "AWS_PROXY"
  integration_uri        = aws_lambda_function.sync.invoke_arn
  payload_format_version = "2.0"
}

resource "aws_apigatewayv2_route" "sync" {
  api_id    = aws_apigatewayv2_api.sync.id
  route_key = "ANY /{proxy+}"
  target    = "integrations/${aws_apigatewayv2_integration.sync.id}"
}

resource "aws_apigatewayv2_stage" "sync" {
  api_id      = aws_apigatewayv2_api.sync.id
  name        = "$default"
  auto_deploy = true
  default_route_settings {
    throttling_burst_limit = 50
    throttling_rate_limit  = 25
  }
  tags = local.common_tags
}

resource "aws_lambda_permission" "api_gateway" {
  statement_id  = "AllowHttpApiInvoke"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.sync.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.sync.execution_arn}/*/*"
}
