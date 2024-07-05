#!/usr/bin/env sh

PLUGIN_DIR=/opt/opsview/monitoringscripts/builtin/plugins
PLUGIN_FILE=$PLUGIN_DIR/check_jitter

# Check if the plugin file exists
if [ ! -f "$PLUGIN_FILE" ]; then
  echo "Plugin file $PLUGIN_FILE does not exist."
  exit 1
fi

# Change ownership and permissions with error handling
if ! chown root:opsview "$PLUGIN_FILE"; then
  echo "Failed to chown $PLUGIN_FILE"
  exit 1
fi

if ! chmod 4550 "$PLUGIN_FILE"; then
  echo "Failed to chmod $PLUGIN_FILE"
  exit 1
fi

echo "Ownership and permissions successfully updated for $PLUGIN_FILE."
