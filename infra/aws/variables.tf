variable "aws_region" {
  description = "AWS region for the Pulse durability layer."
  type        = string
  default     = "us-west-2"
}

variable "name_prefix" {
  description = "Short unique prefix used in AWS resource names."
  type        = string
  default     = "pulse"

  validation {
    condition     = can(regex("^[a-z0-9-]{3,32}$", var.name_prefix))
    error_message = "name_prefix must contain 3-32 lowercase letters, numbers, or hyphens."
  }
}

variable "cockroach_connection_string" {
  description = "CockroachDB PostgreSQL connection string for the Lambda write identity. Never commit this value."
  type        = string
  sensitive   = true

  validation {
    condition     = startswith(var.cockroach_connection_string, "postgres")
    error_message = "cockroach_connection_string must be a PostgreSQL connection string."
  }
}

variable "archive_retention_days" {
  description = "Days to keep raw checkpoint payloads and approved artifact objects."
  type        = number
  default     = 365

  validation {
    condition     = var.archive_retention_days >= 1
    error_message = "archive_retention_days must be at least one day."
  }
}

variable "log_retention_days" {
  description = "CloudWatch log retention period."
  type        = number
  default     = 30
}
