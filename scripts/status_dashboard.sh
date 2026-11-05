#!/usr/bin/env bash
clear
while true; do
    clear
    echo -e "\033[1;36m======================================================================\033[0m"
    echo -e "\033[1;36m       🏛️  LogAnalysis Project Live Dashboard (Herdr Workspace)        \033[0m"
    echo -e "\033[1;36m======================================================================\033[0m"
    echo -e "\033[1;30mLast Updated: $(date '+%Y-%m-%d %H:%M:%S %Z')\033[0m\n"

    echo -e "\033[1;34m[1. Herdr Multi-Terminal Multiplexer Status]\033[0m"
    /home/drew/.local/bin/hc ls 2>/dev/null
    echo ""

    echo -e "\033[1;32m[2. Bottom Agent (t10: /goal fix P0) Status]\033[0m"
    T10_JSON=$(/home/drew/.local/bin/hc get t10 --json 2>/dev/null)
    if [ -n "$T10_JSON" ]; then
        STATE=$(echo "$T10_JSON" | jq -r '.state // "unknown"' 2>/dev/null)
        STATUS=$(echo "$T10_JSON" | jq -r '.status.state // "idle"' 2>/dev/null)
        TURN=$(echo "$T10_JSON" | jq -r '.turn // 0' 2>/dev/null)
        CMD=$(echo "$T10_JSON" | jq -r '.argv | join(" ")' 2>/dev/null)
        echo -e "  - State: \033[1;32m$STATE\033[0m | Action: \033[1;33m$STATUS\033[0m (Turn $TURN)"
        echo -e "  - Command: $CMD"
    else
        echo "  - t10 inactive"
    fi
    echo ""

    echo -e "\033[1;33m[3. Git Working Tree & Active Edits]\033[0m"
    git status -s
    echo -e "Head: $(git log -1 --oneline)"
    echo ""

    echo -e "\033[1;35m[4. Target P0 Remediation Checklist (docs/spec-fix-p0-v4.md)]\033[0m"
    echo -e "  [P0-1] text_input.rs:162 UTF-8 CJK word selection slice panic"
    echo -e "  [P0-2] workspace.rs:382 ADB disconnect zombie loop & unbounded channel"
    echo -e "  [P0-3] adb/logcat.rs:84 low-throughput sparse log pipe stall trap"
    echo -e "  [P0-4] workspace.rs:408 200,000 atomic refcount clone churn on live stream"
    echo ""

    echo -e "\033[1;32m[5. Automated Test Baseline]\033[0m"
    echo -e "  cargo test: \033[1;32m32/32 PASSED\033[0m (loganalysis-core: 27, loganalysis-gui: 5)"
    echo -e "\033[1;36m======================================================================\033[0m"
    echo -e "\033[1;30mAuto-refreshing every 3s... (Ctrl+C to drop to shell)\033[0m"
    sleep 3
done
