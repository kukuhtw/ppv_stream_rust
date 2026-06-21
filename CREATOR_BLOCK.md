# Creator Block and Ban System

This feature lets a creator block a user from purchasing the creator's videos.

## Database

Migration:

```text
migrations/035_creator_blocked_users.sql
```

Table:

```text
creator_blocked_users
```

Important columns:

* `creator_user_id`
* `blocked_user_id`
* `ban_type`
* `reason`
* `expires_at`
* `created_at`

`ban_type` currently supports:

* `soft`
* `hard`

## API

List blocked users:

```http
GET /api/creator/blocked_users
```

Block a user:

```http
POST /api/creator/block_user
```

Request body:

```json
{
  "user_id": "target-user-id",
  "ban_type": "soft",
  "reason": "optional reason"
}
```

Unblock a user:

```http
POST /api/creator/unblock_user
```

Request body:

```json
{
  "user_id": "target-user-id"
}
```

## Enforcement

The migration installs a PostgreSQL trigger on `purchases`.

When a purchase is created, the trigger checks whether the owner of the video has blocked the buyer. If yes, the purchase insert is rejected.

This protects all payment paths that create rows in `purchases`, including wallet, fiat, and X402 flows.

## Current limitation

This first version blocks purchases. It does not yet block chat, profile viewing, free video browsing, comments, follows, refunds, access revocation, or country/IP/device based bans.
