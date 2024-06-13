#!/bin/sh -x

# Infinite loop to keep the program running continuously
while true
do
  # Source the environment variables from the .env file.
  # This allows the program to use updated environment variables without restarting the script.
  source .env

  # Sleep for 2 seconds before starting the program.
  # This short delay gives a moment to review the sourced information above.
  sleep 2

  # Run Argo.
  cargo run --release

  # Sleep for 15 minutes (900 seconds) before restarting the program.
  # This delay can help manage load on the system or external services by preventing constant restarts.
  sleep 900
done
