# Pulse AWS durability layer

This Terraform stack deploys the Workstream 5 cloud boundary: an authenticated
HTTP API, a small Node.js Lambda, private versioned S3 archival, CloudWatch
logs, and a Secrets Manager secret holding the CockroachDB write connection and
the generated Pulse bearer token.

The Lambda accepts the local SQLite outbox at `POST /v1/pulse/sync`, archives
checkpoint payloads under `checkpoints/`, upserts Cockroach rows idempotently,
and writes 384-dimensional MiniLM vectors. `POST /v1/pulse/search` performs
cosine retrieval. `POST /v1/pulse/artifacts/upload-url` returns a 15-minute
pre-signed S3 PUT URL for an explicitly approved artifact; S3 is never public.
When a local artifact has an explicit `local_path`, the desktop sync worker
requests this URL, uploads that file, and syncs its resulting S3 object key.

## Deploy

Use a dedicated CockroachDB **write** SQL user in the connection string. The
desktop app never receives that connection string.

```powershell
cd infra/aws
Copy-Item terraform.tfvars.example terraform.tfvars
# Edit terraform.tfvars with your CockroachDB write connection string.
terraform init
terraform apply

$endpoint = terraform output -raw sync_endpoint
$token = terraform output -raw sync_token
[Environment]::SetEnvironmentVariable("PULSE_SYNC_TOKEN", $token, "User")
```

`terraform apply` runs `npm install --omit=dev` in `lambda/` and packages its
production dependencies. Do not commit `lambda/node_modules`, Terraform state,
or `terraform.tfvars`.

Then put the following in `%LOCALAPPDATA%\Pulse\config.toml` and restart the
Pulse service:

```toml
[sync]
enabled = true
endpoint = "https://replace-with-api-id.execute-api.region.amazonaws.com/v1/pulse/sync"
artifact_bucket = "the-terraform-output-archive_bucket"
token_env = "PULSE_SYNC_TOKEN"
batch_size = 50

[embeddings]
provider = "huggingface_onnx"
model = "sentence-transformers/all-MiniLM-L6-v2"
dimensions = 384
```

Environment variables used by AWS Lambda are set only by Terraform:
`PULSE_SECRET_ARN` and `PULSE_ARCHIVE_BUCKET`. The only desktop environment
variable is `PULSE_SYNC_TOKEN`. The token and CockroachDB connection string
must not be put in `config.toml`, source control, or desktop logs.
