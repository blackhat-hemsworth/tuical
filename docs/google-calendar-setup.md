# Google Calendar Setup Guide

This guide walks you through setting up Google OAuth so TUIcal can access your Google Calendar.

## 1. Create a Google Cloud Project

1. Go to the [Google Cloud Console](https://console.cloud.google.com/)
2. Click the project dropdown at the top and select **New Project**
3. Name it something like `TUIcal` and click **Create**
4. Make sure the new project is selected in the dropdown

## 2. Enable the Google Calendar API

1. Go to **APIs & Services → Library**
2. Search for **Google Calendar API**
3. Click it and press **Enable**

## 3. Configure the OAuth Consent Screen

1. Go to **APIs & Services → OAuth consent screen**
2. Select **External** user type (unless you have a Google Workspace org) and click **Create**
3. Fill in the required fields:
   - **App name:** `TUIcal`
   - **User support email:** your email
   - **Developer contact email:** your email
4. Click **Save and Continue**
5. On the **Scopes** page, click **Add or Remove Scopes** and add:
   - `https://www.googleapis.com/auth/calendar`
   - `https://www.googleapis.com/auth/userinfo.email`
6. Click **Save and Continue** through the remaining steps

> **Note:** While the app is in "Testing" status, only test users you explicitly add (under **OAuth consent screen → Test users**) can authorize. Add your Google account there.

## 4. Create OAuth Client Credentials

1. Go to **APIs & Services → Credentials**
2. Click **Create Credentials → OAuth client ID**
3. Set **Application type** to **TVs and Limited Input devices**
   - This is required for the device code flow that TUIcal uses
   - "Desktop app" will **not** work — it returns `invalid_client_type`
4. Name it `TUIcal` and click **Create**
5. Copy the **Client ID** and **Client Secret**

## 5. Build TUIcal with Credentials

Pass the credentials as environment variables when building:

```sh
GOOGLE_CLIENT_ID="your-client-id.apps.googleusercontent.com" \
GOOGLE_CLIENT_SECRET="your-client-secret" \
cargo build --release
```

The credentials are embedded at compile time via `option_env!()`. The resulting binary does not need the environment variables at runtime.

### Using a `.cargo/config.toml` (optional)

To avoid passing env vars on every build, create `.cargo/config.toml` in the project root:

```toml
[env]
GOOGLE_CLIENT_ID = "your-client-id.apps.googleusercontent.com"
GOOGLE_CLIENT_SECRET = "your-client-secret"
```

> **Do not commit this file.** Add `.cargo/config.toml` to your `.gitignore`.

## 6. Add a Google Calendar in TUIcal

1. Launch `TUIcal`
2. Press `c` to open the Calendar Manager
3. Press `a` to add a calendar
4. Press `2` or `g` to select **Google Calendar**
5. A verification URL and code will appear — press **Enter** to open your browser
6. Sign in with your Google account and enter the code
7. Once authorized, a list of your Google calendars appears
8. Use `j`/`k` to navigate and **Enter** or **Space** to add calendars
9. Press **Esc** when done — your events will load

## Token Storage

OAuth tokens are saved to `~/.config/TUIcal/tokens.json` (or `$XDG_CONFIG_HOME/TUIcal/tokens.json`) with `0600` permissions. Tokens refresh automatically when they expire. To revoke access, delete `tokens.json` or revoke the app at [myaccount.google.com/permissions](https://myaccount.google.com/permissions).

## Troubleshooting

| Error | Cause | Fix |
|---|---|---|
| `Google Calendar not configured` | Binary built without credentials | Rebuild with `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` set |
| `invalid_client_type` | OAuth client is "Desktop app" | Recreate as **"TVs and Limited Input devices"** |
| `User info request failed (401)` | Missing email scope | Rebuild with latest code (scope was added) |
| `access_denied` | User denied consent or not a test user | Add your account under OAuth consent screen → Test users |
| `Token refresh failed` | Refresh token revoked or expired | Delete `tokens.json` and re-authenticate |
