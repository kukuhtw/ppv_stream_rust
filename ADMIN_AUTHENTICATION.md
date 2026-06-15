# Admin Authentication and Password Management

## Overview

The platform uses database-backed authentication for administrator accounts.

Administrator credentials are not validated directly against environment variables during normal login operations.

Authentication is performed using the `users` table and Argon2 password hashes stored in the database.

## Admin Login

Login page:

```text
/public/admin/login.html
```

Login endpoint:

```text
POST /admin/login
```

After successful authentication the administrator is redirected to:

```text
/public/admin/dashboard.html
```

## Password Storage

Passwords are stored in PostgreSQL.

Table:

```text
users
```

Columns:

```text
email
password_hash
is_admin
```

Passwords are hashed using Argon2.

## Authentication Flow

1. User submits email and password.
2. System loads the user record from the database.
3. Argon2 verifies the password against `password_hash`.
4. System verifies `is_admin = true`.
5. A signed session cookie is created.
6. Administrator gains access to the dashboard.

## Bootstrap Administrator

Bootstrap endpoint:

```text
GET /setup_admin
```

Environment variables:

```text
ADMIN_BOOTSTRAP_EMAIL
ADMIN_BOOTSTRAP_PASSWORD
ADMIN_BOOTSTRAP_TOKEN
```

These values are only used for initial administrator creation, promotion, or emergency password reset.

## Change Password

Endpoint:

```text
POST /admin/change_password
```

The application:

1. Verifies the current password.
2. Creates a new Argon2 hash.
3. Updates `users.password_hash`.

Database update:

```sql
UPDATE users
SET password_hash = ?
WHERE id = ?
```

## Does Password Change Update .env?

No.

Changing an administrator password updates only the database.

The `.env` file is never modified by the application.

## Source of Truth

Administrator authentication source:

```text
Database (users.password_hash)
```

Not:

```text
.env
```
