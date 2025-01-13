# API Documentation

## Overview

This API allows devices to:
1. Authenticate and receive a JWT token.
2. Subscribe or unsubscribe from specific topics.
3. Check the status of the service.

### Authentication
- **Endpoint**: `/authenticate`
- **Method**: `POST`
- **Payload**: `{ "device_id": "<DEVICE_ID>" }`
- **Response**: `{ "token": "<JWT_TOKEN>" }`

**Example:**
```bash
curl -X POST http://localhost:8080/authenticate \
     -H "Content-Type: application/json" \
     -d '{ "device_id": "my-device-123" }'
```

### Subscribe to a Topic
- **Endpoint**: `/subscribe`
- **Method**: `POST`
- **Headers**: `Authorization: Bearer <JWT_TOKEN>`
- **Payload**: `{ "topic": "<TOPIC_NAME>" }`
- **Response**: HTTP 200 on success.

**Example:**
```bash
curl -X POST http://localhost:8080/subscribe \
     -H "Authorization: Bearer <JWT_TOKEN>" \
     -H "Content-Type: application/json" \
     -d '{ "topic": "Space" }'
```

### Unsubscribe from a Topic
- **Endpoint**: `/unsubscribe`
- **Method**: `POST`
- **Headers**: `Authorization: Bearer <JWT_TOKEN>`
- **Payload**: `{ "topic": "<TOPIC_NAME>" }`
- **Response**: HTTP 200 on success.

**Example:**
```bash
curl -X POST http://localhost:8080/unsubscribe \
     -H "Authorization: Bearer <JWT_TOKEN>" \
     -H "Content-Type: application/json" \
     -d '{ "topic": "Space" }'
```

### Status Check
- **Endpoint**: `/status`
- **Method**: `POST`
- **Headers**: `Authorization: Bearer <JWT_TOKEN>` (optional)
- **Response**: `"OK"`

**Example (with JWT):**
```bash
curl -X POST http://localhost:8080/status \
     -H "Authorization: Bearer <JWT_TOKEN>"
```

**Example (without JWT):**
```bash
curl -X POST http://localhost:8080/status
```
