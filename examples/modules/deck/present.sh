#!/bin/sh
# Node right-click "Present as slides": open the presenter pane. bohay injects
# BOHAY_BIN_PATH so we call the exact binary that launched us.
exec "${BOHAY_BIN_PATH:-bohay}" module pane open example.deck present
