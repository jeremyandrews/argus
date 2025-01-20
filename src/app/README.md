# API Documentation

## Overview
This API allows devices to:
1. Authenticate and receive a JWT token.
2. Subscribe to or unsubscribe from specific topics.
3. Check the status of the service.

### Authentication
- **Endpoint**: `/authenticate`
- **Method**: `POST`
- **Payload**: `{ "device_id": "<DEVICE_ID>" }`
- **Response**: `{ "token": "<JWT_TOKEN>" }`
- **Note**: The device_id must be a valid 64-character hexadecimal iOS device token.

**Example:**
```bash
curl -X POST http://localhost:8080/authenticate \
     -H "Content-Type: application/json" \
     -d '{ "device_id": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef" }'
```

### Subscribe to a Topic
- **Endpoint**: `/subscribe`
- **Method**: `POST`
- **Headers**: `Authorization: Bearer <JWT_TOKEN>`
- **Payload**: `{ "topic": "<TOPIC_NAME>", "priority": "<PRIORITY>" }`
- **Response**: 
  - HTTP 200 OK on success
  - HTTP 409 CONFLICT if already subscribed
  - HTTP 400 BAD REQUEST for invalid topic
- **Note**: The topic must be one of the predefined valid topics, "Alert", or "Test". Priority is optional.

**Example:**
```bash
curl -X POST http://localhost:8080/subscribe \
     -H "Authorization: Bearer <JWT_TOKEN>" \
     -H "Content-Type: application/json" \
     -d '{ "topic": "Alert", "priority": "high" }'
```

### Unsubscribe from a Topic
- **Endpoint**: `/unsubscribe`
- **Method**: `POST`
- **Headers**: `Authorization: Bearer <JWT_TOKEN>`
- **Payload**: `{ "topic": "<TOPIC_NAME>" }`
- **Response**: 
  - HTTP 200 OK on success
  - HTTP 404 NOT FOUND if subscription doesn't exist
  - HTTP 400 BAD REQUEST for invalid topic
- **Note**: The topic must be one of the predefined valid topics, "Alert", or "Test".

**Example:**
```bash
curl -X POST http://localhost:8080/unsubscribe \
     -H "Authorization: Bearer <JWT_TOKEN>" \
     -H "Content-Type: application/json" \
     -d '{ "topic": "Alert" }'
```

### Status Check
- **Endpoint**: `/status`
- **Method**: `POST`
- **Headers**: `Authorization: Bearer <JWT_TOKEN>` (optional)
- **Response**: 
  - `"OK"` if no JWT is provided or if a valid JWT is provided
  - HTTP 401 UNAUTHORIZED if an invalid JWT is provided

**Example (with JWT):**
```bash
curl -X POST http://localhost:8080/status \
     -H "Authorization: Bearer <JWT_TOKEN>"
```

**Example (without JWT):**
```bash
curl -X POST http://localhost:8080/status
```

## Notes
- All endpoints use POST method.
- JWT tokens are required for subscribe, unsubscribe, and optionally for status check.
- Valid topics are defined by the TOPICS environment variable, plus "Alert" and "Test".
- The server runs on the port specified by the PORT environment variable, defaulting to 8080 if not set.