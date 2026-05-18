# Inbox API

Use this API when an agent needs to inspect a temporary inbox on `smtp.idont.date` over HTTP.

Assume `smtp.idont.date` is an external service that already exposes this API.

Any address at `@idont.date` works without registration or setup. Agents should pick inbox names that are human-readable and specific to the task, with optional short numbering when needed.

Examples:

- `that_subscription_77@idont.date`
- `github_reset_2@idont.date`
- `travel_booking@idont.date`

Prefer names that make it obvious what the inbox is for. This makes later polling and message review easier.

Delivery is not instant. New mail may take at least 10 seconds to appear, and sometimes longer. Agents should assume some delay and use polling with a finite timeout instead of expecting immediate delivery.

## Endpoints

### List messages in an inbox

Request:

```http
GET http://smtp.idont.date/inbox?inbox=<email@domain>&page=1
```

Response:

```json
{
  "mail": [
    {
      "id": 123,
      "date": "2026-05-17 10:00:00.000",
      "recipients": ["that_subscription_77@idont.date"],
      "sender": "<sender@example.com>",
      "subject": "Welcome"
    }
  ],
  "has_more_pages": false
}
```

Use this when you need the current mailbox contents or when you want to find the newest message ID before fetching a full message. Each page contains at most 10 messages. If `page` is omitted, the API returns page 1.

### Read a single message

Request:

```http
GET http://smtp.idont.date/inbox/<id>
```

Response:

```json
{
  "id": 123,
  "date": "2026-05-17 10:00:00.000",
  "recipients": ["that_subscription_77@idont.date"],
  "sender": "<sender@example.com>",
  "subject": "Welcome",
  "body": "Hello from edgemail"
}
```

Use this after the list endpoint tells you which message you want.

## Recommended agent workflow

### Read the inbox

1. Choose a readable inbox name such as `that_subscription_77@idont.date`.
2. Call `GET http://smtp.idont.date/inbox?inbox=<email@domain>&page=1`.
3. Inspect the returned `mail` list by `date` or `id`.
4. Call `GET http://smtp.idont.date/inbox/<id>` for the message you need in full.
5. If `has_more_pages` is `true`, request `page=2`, then `page=3`, and so on until you find what you need or pages are exhausted.

### Wait for a message by polling

Use polling when you expect a message to arrive soon.

1. Choose a readable inbox name such as `that_subscription_77@idont.date`.
2. Call `GET http://smtp.idont.date/inbox?inbox=<email@domain>&page=1` once and record the newest known `id`.
3. Sleep for 5 to 10 seconds.
4. Call `GET http://smtp.idont.date/inbox?inbox=<email@domain>&page=1` again.
5. If a newer `id` appears, fetch it with `GET http://smtp.idont.date/inbox/<id>`.
6. If nothing new appears, keep polling every 5 to 10 seconds until your own task timeout is reached.

Practical guidance:

- Expect delivery to take at least 10 seconds in normal cases, and sometimes longer.
- Prefer a 5 second interval when waiting on an interactive flow like sign-in or verification.
- Prefer a 10 second interval when latency is less important.
- Keep your own overall timeout finite, for example 2 to 5 minutes.
- Compare by `id` or by the first item in the returned `mail` list, since the API returns messages newest first.

## Error handling

- `400` means the request was malformed, for example missing the `inbox` query parameter.
- `404` means the message ID was not found.
- `503` means the API has already served more than 100 requests and will reject further ones.
- `504` means the server-side request timed out after 30 seconds.

If you receive `503`, stop polling and switch to a fresh session or different inbox. If you receive `504`, retry the same request after a short delay.
