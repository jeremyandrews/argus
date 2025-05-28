#!/bin/bash
# Script to diagnose issues with analysis workers
# Run this script on the production server

# Create a timestamp for the output file
TIMESTAMP=$(date +"%Y-%m-%d_%H-%M-%S")
OUTPUT_FILE="analysis_worker_diagnosis_${TIMESTAMP}.log"

echo "Starting analysis worker diagnostics at $(date)" | tee -a "$OUTPUT_FILE"
echo "=======================================================" | tee -a "$OUTPUT_FILE"

# Function to run a command and log its output
run_command() {
  echo "" | tee -a "$OUTPUT_FILE"
  echo "## $1" | tee -a "$OUTPUT_FILE"
  echo "$ $2" | tee -a "$OUTPUT_FILE"
  echo "-------------------------------------------------------" | tee -a "$OUTPUT_FILE"
  eval "$2" 2>&1 | tee -a "$OUTPUT_FILE"
  echo "=======================================================" | tee -a "$OUTPUT_FILE"
}

# 1. Check for errors in the analysis worker logs
run_command "Errors in yesterday's logs" "grep -i \"error\" logs/app.log.2025-05-23 | grep -i \"analysis\" | tail -n 50"
run_command "Errors in today's logs" "grep -i \"error\" logs/app.log.2025-05-24 | grep -i \"analysis\" | tail -n 50"

# 2. Check for LLM connection issues
run_command "Connection issues in yesterday's logs" "grep -i \"connection refused\|failed to connect\|timeout\|connection error\" logs/app.log.2025-05-23 | grep -i \"ollama\|llm\|model\" | tail -n 50"
run_command "Connection issues in today's logs" "grep -i \"connection refused\|failed to connect\|timeout\|connection error\" logs/app.log.2025-05-24 | grep -i \"ollama\|llm\|model\" | tail -n 50"

# 3. Check for model readiness issues
run_command "Model readiness issues in yesterday's logs" "grep -i \"model.*not ready\|failed to become operational\|wait_for_model_ready\" logs/app.log.2025-05-23 | tail -n 50"
run_command "Model readiness issues in today's logs" "grep -i \"model.*not ready\|failed to become operational\|wait_for_model_ready\" logs/app.log.2025-05-24 | tail -n 50"

# 4. Check for worker mode transitions
run_command "Mode transitions in yesterday's logs" "grep -i \"switching to Decision Worker\|switching back from Decision Worker\|idle for more than 10 minutes\" logs/app.log.2025-05-23 | tail -n 50"
run_command "Mode transitions in today's logs" "grep -i \"switching to Decision Worker\|switching back from Decision Worker\|idle for more than 10 minutes\" logs/app.log.2025-05-24 | tail -n 50"

# 5. Check for the last successful processing
run_command "Last successful processing in yesterday's logs" "grep -i \"sent analysis to slack\|pulled from\" logs/app.log.2025-05-23 | tail -n 20"
run_command "Last successful processing in today's logs" "grep -i \"sent analysis to slack\|pulled from\" logs/app.log.2025-05-24 | tail -n 20"

# 6. Check for worker startup and initialization
run_command "Worker startup in yesterday's logs" "grep -i \"starting analysis_loop\" logs/app.log.2025-05-23 | tail -n 10"
run_command "Worker startup in today's logs" "grep -i \"starting analysis_loop\" logs/app.log.2025-05-24 | tail -n 10"

# 7. Check for queue activity
run_command "Queue activity in yesterday's logs" "grep -i \"queue empty\|from.*queue\" logs/app.log.2025-05-23 | tail -n 50"
run_command "Queue activity in today's logs" "grep -i \"queue empty\|from.*queue\" logs/app.log.2025-05-24 | tail -n 50"

# 8. Check for process crashes or restarts
run_command "Process crashes in yesterday's logs" "grep -i \"panic\|crash\|abort\|killed\|terminated\" logs/app.log.2025-05-23 | tail -n 20"
run_command "Process crashes in today's logs" "grep -i \"panic\|crash\|abort\|killed\|terminated\" logs/app.log.2025-05-24 | tail -n 20"

# 9. Check system resources
run_command "System memory usage" "free -h"
run_command "CPU usage" "top -b -n 1 | head -n 20"
run_command "Disk space" "df -h"
run_command "OOM events" "dmesg | grep -i \"out of memory\|oom\""

# 10. Check Ollama service status
run_command "Ollama service status" "systemctl status ollama* || ps aux | grep ollama"
run_command "Ollama logs" "journalctl -u ollama -n 100 --no-pager 2>/dev/null || tail -n 100 /var/log/ollama.log 2>/dev/null || echo 'No Ollama logs found'"

# 11. Check database queue counts
run_command "Database queue counts" "sqlite3 argus.db \"SELECT COUNT(*) FROM life_safety_queue; SELECT COUNT(*) FROM matched_topics_queue; SELECT COUNT(*) FROM rss_queue;\""

echo "Diagnostics completed at $(date)" | tee -a "$OUTPUT_FILE"
echo "Results saved to $OUTPUT_FILE"
echo ""
echo "To restart the analysis workers, you can use the restart_analysis_workers.sh script."
