# Edgemail Inbox API

Use this API when an agent needs to inspect a temporary inbox on `smtp.idont.date` over HTTP.

Assume `smtp.idont.date` is an external service that already exposes this API.

## Endpoints

### List messages in an inbox

Request:

```http
GET https://smtp.idont.date/inbox?inbox=<email@domain>
```

Response:

```json
[
  {
    "id": 123,
    "date": "2026-05-17 10:00:00.000",
    "recipients": ["agent_123@smtp.idont.date"],
    "sender": "<sender@example.com>",
    "subject": "Welcome"
  }
]
```

Use this when you need the current mailbox contents or when you want to find the newest message ID before fetching a full message.

### Read a single message

Request:

```http
GET https://smtp.idont.date/inbox/<id>
```

Response:

```json
{
  "id": 123,
  "date": "2026-05-17 10:00:00.000",
  "recipients": ["agent_123@smtp.idont.date"],
  "sender": "<sender@example.com>",
  "subject": "Welcome",
  "body": "Hello from edgemail"
}
```

Use this after the list endpoint tells you which message you want.

## Recommended agent workflow

### Read the inbox

1. Call `GET https://smtp.idont.date/inbox?inbox=<email@domain>`.
2. Sort or inspect the returned entries by `date` or `id`.
3. Call `GET https://smtp.idont.date/inbox/<id>` for the message you need in full.

### Wait for a message by polling

Use polling when you expect a message to arrive soon.

1. Call `GET https://smtp.idont.date/inbox?inbox=<email@domain>` once and record the newest known `id`.
2. Sleep for 5 to 10 seconds.
3. Call `GET https://smtp.idont.date/inbox?inbox=<email@domain>` again.
4. If a newer `id` appears, fetch it with `GET https://smtp.idont.date/inbox/<id>`.
5. If nothing new appears, keep polling every 5 to 10 seconds until your own task timeout is reached.

Practical guidance:

- Prefer a 5 second interval when waiting on an interactive flow like sign-in or verification.
- Prefer a 10 second interval when latency is less important.
- Keep your own overall timeout finite, for example 2 to 5 minutes.
- Compare by `id` or by the first item in the returned list, since the API returns messages newest first.

## Error handling

- `400` means the request was malformed, for example missing the `inbox` query parameter.
- `404` means the message ID was not found.
- `503` means the API has already served more than 100 requests and will reject further ones.
- `504` means the server-side request timed out after 30 seconds.

If you receive `503`, stop polling and restart the server or switch to a fresh session. If you receive `504`, retry the same request after a short delay.
