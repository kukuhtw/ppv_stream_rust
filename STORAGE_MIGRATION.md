# Storage Migration Guide

This document explains how to migrate the platform from local filesystem storage to an S3-compatible backend such as:

- AWS S3
- MinIO
- Cloudflare R2
- Backblaze B2
- other S3-compatible object storage platforms

It is written for operators and administrators who want a practical, safe migration path.

-> [README.md](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/README.md) | [SETUP.md](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/SETUP.md) | [TECHNICAL_DOCUMENTATION.md](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/TECHNICAL_DOCUMENTATION.md)

## Important Reality Check

This repository already includes a storage plugin abstraction and an S3-compatible provider implementation.

Relevant code:

- [src/plugins/storage/traits.rs](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/src/plugins/storage/traits.rs)
- [src/plugins/storage/registry.rs](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/src/plugins/storage/registry.rs)
- [src/plugins/storage/providers/local.rs](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/src/plugins/storage/providers/local.rs)
- [src/plugins/storage/providers/s3.rs](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/src/plugins/storage/providers/s3.rs)

However, the current architecture is still best described as:

- local-first processing
- optional remote sync
- not yet fully remote-native playback

That means:

1. uploaded originals are still written to local disk first
2. transcoded HLS output is still generated on local disk first
3. playback sessions under `HLS_ROOT` are still served from local filesystem
4. remote object storage is currently used as a storage backend and sync target, not as a full replacement for every local runtime path

So this migration guide is realistic:

- it helps you enable S3 or MinIO safely
- it helps you migrate existing files
- it does not pretend that you can immediately delete all local storage paths without further code changes

## Admin UI Support

The repository now includes an admin-facing storage workflow in:

- `Admin > Settings > Storage Backend and Migration`

From that UI, an admin can:

- save the desired storage backend configuration in the database
- test connectivity to the configured backend
- compare the saved desired backend with the currently active runtime backend
- start a background migration job for uploaded originals, transcoded HLS media, or both
- monitor recent migration jobs and progress
- cancel a running job
- resume a failed, cancelled, or completed-with-errors job
- inspect file-level migration records
- filter job item details to focus on failed or retried records

Important:

- the runtime backend still comes from environment variables at application startup
- saving a new desired backend in the admin UI does not hot-switch the runtime backend immediately
- a restart is still required when the saved backend and active runtime backend differ

See also:

- [STORAGE_ADMIN_MOCKUP.md](STORAGE_ADMIN_MOCKUP.md)

## Admin Tutorial

This is the recommended operator workflow for the current admin UI.

### A. Save and verify the backend

1. open `Admin > Settings > Storage Backend and Migration`
2. review the `Active Storage Backend` card
3. fill in the desired backend form
4. click `Save Storage Settings`
5. click `Test Connection`
6. if the page shows `Restart required: Yes`, restart the application before expecting the runtime backend to change

### B. Start a first migration job

1. keep local runtime directories in place
2. choose whether to migrate uploaded originals, transcoded media, or both
3. click `Start Migration`
4. watch the progress bar, retry count, and last error column in the recent jobs table
5. use `View Items` if you need file-level detail

### C. Inspect item-level detail

1. click `View Items` on a job row
2. use the filter selector to narrow the list:
   - `All items`
   - `Failed only`
   - `Retried only`
   - `Failed or retried`
3. review:
   - source path
   - destination object key
   - retry count
   - final error message if present

### D. Cancel a running job

1. click `Cancel` on a running job
2. the status changes to `cancel_requested`
3. once the current in-flight upload finishes, the job should transition to `cancelled`

### E. Resume a partial job

1. find a job with status `failed`, `cancelled`, or `completed_with_errors`
2. click `Resume`
3. a new job is created with a `Resume of <job_id>` note
4. object keys previously marked as `copied` in the source job are recorded as `skipped` in the new job
5. inspect `View Items` if you want to confirm exactly which files were skipped, retried, copied, or failed

## What Can Be Migrated Today

With the current codebase, the following storage patterns are supported:

### Original uploaded files

Uploaded files can be pushed to remote storage after upload.

Example behavior:

- upload writes to local filesystem
- backend optionally uploads the original file to remote object storage

Relevant code:

- [src/handlers/upload.rs](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/src/handlers/upload.rs:406)

### Transcoded HLS output

The worker can push generated HLS output directories to remote storage after transcoding finishes.

Relevant code:

- [src/worker.rs](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/src/worker.rs:159)

### What still remains local at runtime

- upload temporary files
- transcoding temp files
- local media working directories
- per-session HLS playback directories under `HLS_ROOT`
- direct `/hls/:session/:file` serving logic

Relevant code:

- [src/handlers/stream.rs](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/src/handlers/stream.rs:513)
- [src/config.rs](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/src/config.rs:20)

## When You Should Migrate

Migrating to S3 or MinIO is useful when:

- you want more durable storage than a single VPS disk
- you want simpler off-site backup strategy
- you want to prepare for CDN or object-storage-based delivery later
- you want easier multi-environment or multi-node asset persistence
- you want to separate application compute from media persistence

You may not need to migrate yet if:

- you are still in local development
- you are validating the product on a single machine
- your catalog is still small
- you do not yet need external object storage or CDN

## Supported Backends

The storage registry recognizes these `STORAGE_BACKEND` values:

- `local`
- `s3`
- `minio`
- `r2`
- `b2`

The `s3` provider implementation is shared for all S3-compatible backends.

## Storage Layout Used by the Current Code

Before migrating, understand the current local directories:

- `STORAGE_DIR` or `UPLOAD_DIR`
  original uploaded files
- `MEDIA_DIR`
  persistent transcoded HLS output
- `HLS_ROOT`
  temporary per-session playback HLS
- `TMP_DIR`
  temporary processing files

Common defaults:

```env
STORAGE_DIR=storage
UPLOAD_DIR=storage
MEDIA_DIR=media
HLS_ROOT=hls_tmp
TMP_DIR=tmp
```

### Remote object key patterns

The current code uses these prefixes:

- original uploaded files:
  `uploads/<filename>`
- transcoded HLS outputs:
  `videos/<video_id>/...`

These prefixes matter when you manually backfill old assets.

## Migration Strategy

The safest migration strategy is:

1. keep local storage active
2. enable remote backend
3. let new uploads and new transcodes sync to remote
4. backfill old assets from local to remote
5. verify object completeness
6. keep local runtime directories for upload, transcode, and playback

This avoids risky big-bang cutovers.

## Step-by-Step Migration Plan

## 1. Prepare a Full Backup

Before changing anything, back up:

- database
- `storage/`
- `media/`
- `hls_tmp/`
- `tmp/`
- `.env`

Recommended minimum backup set:

- PostgreSQL dump
- original uploaded files
- transcoded media
- current application configuration

## 2. Inventory Your Existing Files

Estimate how much data you have and where it lives.

Typical inventory questions:

- how many uploaded originals exist under `storage/`
- how many transcoded video directories exist under `media/`
- how large is the total media footprint
- which files are active versus stale

Example checks on Linux or macOS:

```bash
du -sh storage media hls_tmp tmp
find storage -type f | wc -l
find media -type f | wc -l
```

Example checks on Windows PowerShell:

```powershell
Get-ChildItem storage -Recurse -File | Measure-Object
Get-ChildItem media -Recurse -File | Measure-Object
```

## 3. Create the Target Bucket

Create a bucket or container in your chosen object storage.

Examples:

- `ppv-stream-prod`
- `ppv-stream-media`
- `brand-a-video-assets`

Decide:

- bucket name
- region
- credentials
- public URL or CDN URL if applicable
- whether path-style URLs are required

For MinIO, path-style is usually required.

## 4. Configure Environment Variables

Update your `.env` or deployment secrets.

### Example for MinIO

```env
STORAGE_BACKEND=minio
S3_BUCKET=ppv-stream
S3_REGION=us-east-1
S3_ACCESS_KEY=minioadmin
S3_SECRET_KEY=minioadmin
S3_ENDPOINT=http://minio.example.internal:9000
S3_PATH_STYLE=true
S3_PUBLIC_URL=https://files.example.com/ppv-stream
```

### Example for AWS S3

```env
STORAGE_BACKEND=s3
S3_BUCKET=ppv-stream-prod
S3_REGION=ap-southeast-1
S3_ACCESS_KEY=YOUR_ACCESS_KEY
S3_SECRET_KEY=YOUR_SECRET_KEY
S3_PUBLIC_URL=https://ppv-stream-prod.s3.ap-southeast-1.amazonaws.com
```

### Example for Cloudflare R2

```env
STORAGE_BACKEND=r2
S3_BUCKET=ppv-stream-prod
S3_REGION=auto
S3_ACCESS_KEY=YOUR_R2_ACCESS_KEY
S3_SECRET_KEY=YOUR_R2_SECRET_KEY
S3_ENDPOINT=https://<accountid>.r2.cloudflarestorage.com
S3_PATH_STYLE=true
S3_PUBLIC_URL=https://cdn.example.com
```

## 5. Keep Local Directories in Place

Even after enabling remote storage, do not remove these local directories:

- `storage`
- `media`
- `hls_tmp`
- `tmp`

Why:

- uploads still land on local disk first
- transcode still runs on local disk
- per-session playback still serves local files

This is the most important operational caveat.

## 6. Restart the Application

After updating environment variables, restart the app.

At startup, the storage registry should select the configured backend.

Relevant code:

- [src/plugins/storage/registry.rs](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/src/plugins/storage/registry.rs)

Check application logs for lines indicating:

- selected storage backend
- endpoint or bucket information
- fallback to local if initialization failed

If initialization fails, the current code falls back to local storage.
That is convenient for uptime, but dangerous if you expected remote storage to be active.

So always verify the logs after restart.

If you use the admin storage settings page first, also verify that:

- the saved desired backend is correct
- the active runtime backend matches it after restart

## 7. Test New Uploads First

Before migrating old files, test the new backend using a fresh upload.

Recommended test:

1. upload a small MP4
2. wait until transcoding completes
3. verify that the original appears in remote storage under:
   `uploads/<filename>`
4. verify that HLS output appears under:
   `videos/<video_id>/`
5. verify that playback still works in the browser

This proves:

- credentials work
- bucket access works
- remote sync works
- the main user journey still works

## 8. Backfill Existing Original Uploads

Once new uploads work, migrate old originals from local storage to remote storage.

Target object key pattern:

```text
uploads/<filename>
```

### Example using AWS CLI for S3

```bash
aws s3 cp storage/ s3://ppv-stream-prod/uploads/ --recursive
```

### Example using `mc` for MinIO

```bash
mc alias set myminio http://minio.example.internal:9000 minioadmin minioadmin
mc cp --recursive storage/ myminio/ppv-stream/uploads/
```

### Example using `rclone`

```bash
rclone copy storage remote:ppv-stream/uploads --progress
```

Important:

- do not change filenames unless you also change how the application resolves them
- preserve object paths exactly

## 9. Backfill Existing HLS Output

Migrate persistent HLS output from local `media` directories to the remote `videos/<video_id>/` structure.

Target object key pattern:

```text
videos/<video_id>/<relative-file>
```

This usually means each subdirectory under your media root should map to one `video_id`.

If your local media directory structure is:

```text
media/
  <video_id>/
    master.m3u8
    360p.m3u8
    seg_000.ts
```

then the remote structure should become:

```text
videos/<video_id>/master.m3u8
videos/<video_id>/360p.m3u8
videos/<video_id>/seg_000.ts
```

### Example using AWS CLI

```bash
for d in media/*; do
  id=$(basename "$d")
  aws s3 cp "$d" "s3://ppv-stream-prod/videos/$id/" --recursive
done
```

### Example using PowerShell

```powershell
Get-ChildItem media -Directory | ForEach-Object {
  $videoId = $_.Name
  aws s3 cp $_.FullName "s3://ppv-stream-prod/videos/$videoId/" --recursive
}
```

## 10. Validate Remote Object Completeness

Do not assume sync succeeded just because the copy command exited successfully.

Check:

- object counts
- sample files
- random video directories
- file sizes
- playlist and segment presence

Recommended validation:

1. compare local file count vs remote object count for `storage`
2. compare local file count vs remote object count for `media`
3. open several remote URLs manually if `S3_PUBLIC_URL` is configured
4. test with recent, old, and edge-case videos

## 11. Run Functional Tests

After migration, manually test these flows:

1. register and login
2. upload a new video
3. wait for transcoding
4. open the watch page
5. purchase via wallet if enabled
6. purchase via x402 if enabled
7. purchase via fiat provider if enabled
8. verify playback works
9. verify original and HLS assets appear in remote object storage
10. verify the admin `Test Connection` action succeeds
11. verify the storage migration job table shows the expected status and progress
12. if you test job cancellation, verify the status changes to `cancel_requested` and then `cancelled`

What you are really testing:

- remote sync did not break upload
- remote sync did not break transcode
- local runtime paths still work
- purchase and entitlement logic still work

## 12. Keep Local Runtime Storage After Migration

This deserves to be repeated clearly:

Do not remove local runtime directories after enabling S3 or MinIO.

At minimum, you still need local working storage for:

- uploads
- temp processing
- worker output
- session playback HLS generation

If you remove local disk paths too early, the application may fail during:

- upload
- transcode
- playback request generation
- per-session HLS serving

## 13. Optional Hybrid Operating Model

A good production compromise is:

- local disk for runtime working files
- object storage for durable persistence
- optional CDN in front of object storage for future delivery patterns

This model is often enough for:

- single-node production
- backup improvement
- disaster recovery improvement
- storage scaling preparation

without requiring immediate code refactors.

## 14. Rollback Plan

If something goes wrong, rollback is straightforward.

### Rollback steps

1. set `STORAGE_BACKEND=local`
2. keep all local directories intact
3. restart the application
4. verify uploads and playback work locally again

Because the current system is local-first internally, rollback is relatively safe if you preserved the original local files.

## 15. Known Limitations of the Current Architecture

The following limitations exist today:

### No hot runtime backend switch

The repo now includes admin UI for saved storage settings and background migration jobs.
However, the actual runtime backend used by upload and worker components is still initialized from environment variables during application startup.

So a restart is still required to fully apply a backend change at runtime.

### Built-in migration job is still basic

The repo now includes a basic built-in background migration workflow.
However, it is still a first-generation implementation and does not yet provide:

- distributed locking across multiple application instances
- distributed cancellation coordination across multiple application instances
- full resumable checkpoints with orchestration across repeated job chains
- advanced retry policies with configurable backoff and per-provider tuning
- object checksum validation
- full database-to-object reconciliation

The current admin `Cancel` action is process-local.
That means it works correctly for a single application instance, but it is not yet designed as a cluster-wide cancellation signal.

The current worker does include a basic built-in retry for each object upload attempt.
This helps with short-lived network interruptions, but it is intentionally conservative and should not be treated as a substitute for full resumable migration orchestration.

The admin workflow now also supports a basic resume mode.
When a prior job ended as `failed`, `cancelled`, or `completed_with_errors`, operators can start a new resume job that skips object keys already recorded as successfully copied in the source job.
This is a practical checkpoint layer, but it still depends on the recorded job item history rather than a full content-verification or distributed checkpoint system.

The admin migration table now also shows the cumulative retry count for each job.
This gives operators a lightweight signal that a migration completed under unstable network conditions, even when the final status is still `completed`.

The admin screen also now exposes file-level migration records for each job.
Operators can inspect up to the most recent 200 item records, including source path, destination object key, retry count, status, and any final error message.
The detail panel also supports quick filtering for failed items, retried items, or a combined failed-or-retried view.

### Playback is still local

Per-session playback still depends on local HLS session directories.

### No remote-first stream serving

The app does not yet serve main playback assets directly from S3 or MinIO using:

- signed URLs
- CDN URLs
- remote object reads in the HLS handler

### No storage health dashboard

There is no admin screen for:

- testing bucket connectivity
- browsing object sync state
- detecting missing objects

## 16. When You Need Additional Engineering Work

You should plan a code refactor if your real goal is:

- fully stateless application nodes
- multi-node horizontal scaling for media delivery
- remote-first stream serving
- CDN-backed VOD delivery
- no dependency on local media directories except temp files
- storage switching from admin dashboard

That future refactor would typically include:

1. storage settings persisted outside raw env-only config
2. migration job management and progress tracking
3. object existence verification
4. remote fetch fallback for missing local files
5. optional presigned URL flow
6. optional CDN or edge caching strategy
7. cleaner split between persistent VOD assets and session watermark assets

## 17. Recommended Production Advice

For most teams, the safest path is:

1. start with local storage
2. add S3 or MinIO as a sync target
3. keep local runtime storage
4. backfill old files
5. verify regularly
6. only later refactor toward remote-first delivery if scale actually requires it

This gives you most of the operational benefit with much less migration risk.

## 18. Quick Checklist

Use this short checklist during migration:

- backup database
- backup `storage`, `media`, `hls_tmp`, `tmp`
- create bucket
- configure env vars
- restart app
- verify log shows correct backend
- upload one new video
- confirm original object exists remotely
- confirm HLS output exists remotely
- backfill old originals
- backfill old HLS output
- validate counts and sample files
- keep local runtime directories
- document rollback procedure

## 19. Final Summary

Yes, this platform can be migrated from local storage to S3, MinIO, or other S3-compatible platforms.

But with the current codebase, the migration should be understood as:

- enabling remote persistence and sync
- not eliminating all local storage behavior

If you follow a hybrid migration approach, you can get:

- safer backups
- better durability
- easier future scaling

without breaking upload, transcode, purchase, or playback flows.
