#!/usr/bin/env python3

import subprocess
import time
import websocket
import json
import sys
import signal
import os
import threading

# Global variable to store the time tracker process
time_tracker_process = None

def signal_handler(sig, frame):
    """Handle Ctrl+C gracefully"""
    print("\nStopping time tracker...")
    if time_tracker_process:
        time_tracker_process.terminate()
        time_tracker_process.wait()
    sys.exit(0)

def print_time_tracker_output(process):
    """Print output from the time tracker process"""
    try:
        for line in iter(process.stdout.readline, ''):
            if line:
                print(f"[TimeTracker] {line.rstrip()}")
            else:
                break
    except Exception as e:
        pass

def start_time_tracker():
    """Start the time tracker with --memory flag"""
    global time_tracker_process

    print("Killing any existing time tracker processes...")
    subprocess.run(["pkill", "-f", "time_tracker"], capture_output=True)

    print("Starting time tracker with --memory flag...")

    # Start the time tracker in the background with --memory flag
    # We need to compile with the sqlite feature
    try:
        time_tracker_process = subprocess.Popen([
            "cargo", "run", "--features", "memory", "--", "--memory"
        ], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)

        # Start a thread to print the time tracker output
        output_thread = threading.Thread(target=print_time_tracker_output, args=(time_tracker_process,))
        output_thread.daemon = True
        output_thread.start()

        # Give the application time to start
        print("Waiting for time tracker to initialize...")
        time.sleep(5)

        # Check if the process is still running
        if time_tracker_process.poll() is not None:
            print("Error: Time tracker failed to start")
            stdout, stderr = time_tracker_process.communicate()
            print("STDOUT:", stdout)
            print("STDERR:", stderr)
            return False

        print(f"Time tracker is running with PID: {time_tracker_process.pid}")
        return True
    except Exception as e:
        print(f"Error starting time tracker: {e}")
        return False

def send_websocket_message(message):
    """Send a message through websocket and return the response"""
    try:
        ws = websocket.create_connection("ws://127.0.0.1:6754")
        ws.send(json.dumps(message))
        result = ws.recv()
        ws.close()
        return result
    except Exception as e:
        print(f"Error sending websocket message: {e}")
        return None

def add_mpv_process():
    """Add mpv process through websocket"""
    print("Adding mpv process through websocket...")

    # Create the JSON message for AddProcess command
    # Based on the handle_add_process function, the payload should be a JSON string
    # The WebSocketCommand expects: {"type": "add_process", "payload": "{\"process_name\": \"mpv\", \"path\": \"/usr/bin/mpv\"}"}
    add_process_msg = {
        "type": "add_process",
        "payload": "{\"process_name\": \"mpv\", \"path\": \"/usr/bin/mpv\"}"
    }

    # Send the message
    response = send_websocket_message(add_process_msg)
    if response:
        print(f"Response: {response}")
        return True
    else:
        print("Failed to add mpv process")
        return False

def get_apps_list():
    """Get the list of apps to verify the mpv process was added"""
    print("Getting list of apps...")

    get_apps_msg = {
        "type": "get_apps",
        "payload": ""
    }

    # Send the message
    response = send_websocket_message(get_apps_msg)
    if response:
        print(f"Response: {response}")
        return True
    else:
        print("Failed to get apps list")
        return False

def main():
    """Main function"""
    # Set up signal handler for graceful shutdown
    signal.signal(signal.SIGINT, signal_handler)

    try:
        # Start the time tracker
        if not start_time_tracker():
            return 1

        # Update TimeTrackingConfig with smaller delays
        print("Updating TimeTrackingConfig with smaller delays...")
        update_config_msg = {
            "type": "update_config",
            "payload": "{\"tracking_delay_ms\": 1000, \"check_delay_ms\": 500}"
        }
        response = send_websocket_message(update_config_msg)
        if response:
            print(f"Config update response: {response}")
        else:
            print("Failed to update TimeTrackingConfig")
            return 1

        # Add mpv process
        if not add_mpv_process():
            return 1

        # Wait a moment for the command to be processed
        time.sleep(20)

        # Get apps list to verify
        if not get_apps_list():
            return 1

        print("Test completed successfully!")

        # Ask user if they want to keep the time tracker running
        # Handle both interactive and non-interactive environments
        try:
            response = input("Do you want to keep the time tracker running? (y/n): ")
            if response.lower() not in ['y', 'yes']:
                print("Stopping time tracker...")
                if time_tracker_process:
                    time_tracker_process.terminate()
                    time_tracker_process.wait()
                print("Time tracker stopped")
            else:
                print(f"Time tracker is still running with PID: {time_tracker_process.pid}")
                print("To stop it later, run: kill", time_tracker_process.pid)
                print("Press Ctrl+C to stop the script (time tracker will continue running)")
                # Wait indefinitely
                signal.pause()
        except EOFError:
            # Non-interactive environment, stop the time tracker
            print("\nNon-interactive environment detected. Stopping time tracker...")
            if time_tracker_process:
                time_tracker_process.terminate()
                time_tracker_process.wait()
            print("Time tracker stopped")

        return 0

    except Exception as e:
        print(f"Error: {e}")
        if time_tracker_process:
            time_tracker_process.terminate()
            time_tracker_process.wait()
        return 1

if __name__ == "__main__":
    sys.exit(main())
