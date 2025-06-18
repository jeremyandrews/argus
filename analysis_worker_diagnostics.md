# Analysis Worker Diagnostics

This document contains commands to diagnose issues with the analysis workers on the production server. The commands are designed for Linux and target the log files in the `logs/` directory.

## 1. Check for errors in the analysis worker logs

```bash
# Check for errors in yesterday's logs
grep -i "error" logs/app.log.2025-05-23 | grep -i "analysis" | tail -n 50

# Check for errors in today's logs
grep -i "error" logs/app.log.2025-05-24 | grep -i "analysis" | tail -n 50
```

## 2. Check for LLM connection issues

```bash
# Check for connection issues in yesterday's logs
grep -i "connection refused\|failed to connect\|timeout\|connection error" logs/app.log.2025-05-23 | grep -i "ollama\|llm\|model" | tail -n 50

# Check for connection issues in today's logs
grep -i "connection refused\|failed to connect\|timeout\|connection error" logs/app.log.2025-05-24 | grep -i "ollama\|llm\|model" | tail -n 50
```

## 3. Check for model readiness issues

```bash
# Check for model readiness issues in yesterday's logs
grep -i "model.*not ready\|failed to become operational\|wait_for_model_ready" logs/app.log.2025-05-23 | tail -n 50

# Check for model readiness issues in today's logs
grep -i "model.*not ready\|failed to become operational\|wait_for_model_ready" logs/app.log.2025-05-24 | tail -n 50
```

## 4. Check for worker mode transitions

```bash
# Check for mode transitions in yesterday's logs
grep -i "switching to Decision Worker\|switching back from Decision Worker\|idle for more than 10 minutes" logs/app.log.2025-05-23 | tail -n 50

# Check for mode transitions in today's logs
grep -i "switching to Decision Worker\|switching back from Decision Worker\|idle for more than 10 minutes" logs/app.log.2025-05-24 | tail -n 50
```

## 5. Check for the last successful processing

```bash
# Find the last successful processing in yesterday's logs
grep -i "sent analysis to slack\|pulled from" logs/app.log.2025-05-23 | tail -n 20

# Find the last successful processing in today's logs
grep -i "sent analysis to slack\|pulled from" logs/app.log.2025-05-24 | tail -n 20
```

## 6. Check for worker startup and initialization

```bash
# Check worker startup in yesterday's logs
grep -i "starting analysis_loop" logs/app.log.2025-05-23 | tail -n 10

# Check worker startup in today's logs
grep -i "starting analysis_loop" logs/app.log.2025-05-24 | tail -n 10
```

## 7. Check for queue activity

```bash
# Check queue activity in yesterday's logs
grep -i "queue empty\|from.*queue" logs/app.log.2025-05-23 | tail -n 50

# Check queue activity in today's logs
grep -i "queue empty\|from.*queue" logs/app.log.2025-05-24 | tail -n 50
```

## 8. Check for process crashes or restarts

```bash
# Check for process crashes in yesterday's logs
grep -i "panic\|crash\|abort\|killed\|terminated" logs/app.log.2025-05-23 | tail -n 20

# Check for process crashes in today's logs
grep -i "panic\|crash\|abort\|killed\|terminated" logs/app.log.2025-05-24 | tail -n 20
```

## 9. Check system resources

```bash
# Check system memory and CPU usage
free -h
top -b -n 1 | head -n 20

# Check disk space
df -h

# Check for any OOM (Out of Memory) events
dmesg | grep -i "out of memory\|oom"
```

## 10. Check Ollama service status

```bash
# Check if Ollama services are running
systemctl status ollama* || ps aux | grep ollama

# Check Ollama logs if available
journalctl -u ollama -n 100 --no-pager || tail -n 100 /var/log/ollama.log
```

## 11. Create a shell script to restart analysis workers

If you need to restart the analysis workers after diagnosing the issue:

```bash
#!/bin/bash
# Save as restart_analysis_workers.sh

echo "Stopping analysis workers..."
pkill -f "analysis_worker" || echo "No analysis workers found to kill"

echo "Waiting for processes to terminate..."
sleep 5

echo "Starting analysis workers..."
cd /path/to/argus
nohup ./target/release/argus --worker-type analysis > logs/analysis_worker.log 2>&1 &

echo "Analysis workers restarted. Check logs for startup messages."
```

Make the script executable with `chmod +x restart_analysis_workers.sh` before running it.
