# Storage Admin Mockup

This file documents the intended admin experience for storage configuration and migration monitoring.

Related docs:

- [README.md](README.md)
- [SETUP.md](SETUP.md)
- [STORAGE_MIGRATION.md](STORAGE_MIGRATION.md)

## Purpose

The storage admin area is designed to help an operator:

1. save the desired storage backend configuration
2. test storage connectivity before switching workflows
3. compare active runtime storage vs saved desired storage
4. migrate existing local files into the remote backend
5. cancel a running job when needed
6. resume a failed or cancelled job without re-copying files already marked as copied
7. inspect file-level migration results

## Menu Location

The workflow lives in:

- `Admin > Settings > Storage Backend and Migration`

## High-Level Layout

```text
+----------------------------------------------------------------------------------+
| Admin Settings                                                                   |
+----------------------------------------------------------------------------------+
| Active Storage Backend                                                           |
| Backend: local                                                                   |
| Bucket: -                                                                        |
| Endpoint: -                                                                      |
| Public URL: -                                                                    |
+----------------------------------------------------------------------------------+
| Desired Storage Backend                                                          |
| Backend: minio                                                                   |
| Bucket: ppv-stream                                                               |
| Endpoint: http://minio:9000                                                      |
| Missing fields: Complete                                                         |
| Restart required: Yes                                                            |
| Warning: saved backend differs from runtime backend until app restart            |
+----------------------------------------------------------------------------------+
| Storage Settings Form                                                            |
| Backend      [ minio                    ]                                        |
| Bucket       [ ppv-stream               ]                                        |
| Region       [ us-east-1                ]                                        |
| Access Key   [ minioadmin               ]                                        |
| Secret Key   [ ********                 ]                                        |
| Endpoint     [ http://minio:9000        ]                                        |
| Public URL   [ https://files.example    ]                                        |
| Path Style   [x] Force path-style requests                                      |
| [Save Storage Settings] [Test Connection]                                        |
+----------------------------------------------------------------------------------+
| Storage Migration Job                                                            |
| [x] Copy uploaded originals                                                      |
| [x] Copy transcoded HLS media                                                    |
| [Start Migration]                                                                |
+----------------------------------------------------------------------------------+
| Recent Jobs                                                                      |
| Created | Status | Backend | Scope | Progress | Last Error | Actions            |
| ------- | ------ | ------- | ----- | -------- | ---------- | ------------------ |
| 10:00   | running| minio   | both  | 52/80    | -          | View Items Cancel  |
| 09:30   | failed | minio   | media | 44/80    | timeout    | View Items Resume  |
| 08:40   | done   | minio   | both  | 80/80    | -          | View Items         |
+----------------------------------------------------------------------------------+
| Job Item Details                                                                 |
| Filter: [ Failed or retried ]                                                    |
| Visible: 12 of 200 items. Copied: 170. Failed: 8. Retried: 24.                 |
| Created | Scope | Status  | Retries | Source Path | Object Key | Error          |
| ------- | ----- | ------- | ------- | ----------- | ---------- | -------------- |
| 10:02   | media | failed  | 2       | media/...   | videos/... | timeout        |
| 10:02   | media | skipped | 0       | media/...   | videos/... | resumed skip   |
+----------------------------------------------------------------------------------+
```

## Card-by-Card Behavior

### Active Storage Backend

This card reflects the backend currently active in the running process.

It is derived from environment variables such as:

- `STORAGE_BACKEND`
- `S3_BUCKET`
- `S3_REGION`
- `S3_ENDPOINT`
- `S3_PUBLIC_URL`
- `S3_PATH_STYLE`

This section answers:

- what backend the app is actually using right now
- whether a restart is still needed after saving a different desired backend

### Desired Storage Backend

This card reflects the saved storage settings stored in the database.

It helps the admin see:

- the saved backend target
- whether the configuration is complete
- whether runtime and desired state are different

### Storage Settings Form

This form lets the admin save a remote backend profile without immediately changing the active runtime backend.

Expected flow:

1. choose backend
2. enter bucket and credentials
3. save settings
4. test connection
5. restart the application if runtime backend should change

### Storage Migration Job

This section starts a background migration job.

Scope options:

- uploaded originals only
- transcoded media only
- both

### Recent Jobs

This table gives operators a short operational summary.

Expected status values:

- `pending`
- `running`
- `cancel_requested`
- `cancelled`
- `completed`
- `completed_with_errors`
- `failed`

Expected action buttons:

- `View Items`
- `Cancel`
- `Resume`

### Job Item Details

This panel is for troubleshooting and audit.

It shows file-level outcomes such as:

- `copied`
- `failed`
- `skipped`

The `skipped` status is especially important for resumed jobs, because it indicates an object key that was already marked as successfully copied in the source job used for resume.

## Resume Job UX

The `Resume` action should feel like a continuation, not a brand-new blind retry.

Expected operator mental model:

1. a job fails or is cancelled
2. the operator reviews the failed items
3. the operator clicks `Resume`
4. the new job references the previous job id
5. already copied object keys are skipped
6. only remaining files are attempted again

In the jobs table, the resumed job should visibly show:

- `Resume of <job_id>`

This makes job chains understandable during support and audit.

## Mockup Notes

This mockup is intentionally operational rather than decorative.

The most important UX goals are:

- clear visibility into active vs desired state
- safe migration controls
- transparent progress reporting
- practical failure recovery
- low-friction audit of file-level outcomes
