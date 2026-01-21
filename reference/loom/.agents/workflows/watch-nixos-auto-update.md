Your task is to every minute:

a. spawn a subagent that monitors for deployment failures and if a deployment failure happens related to cargo2nix then your task is to "git pull trunk", then regenerate cargo2nix (see agents.md and flakes.nix), then commit and push the fix.

IMPORTANT: 
a. You are on the server, you can inspect logs via journald for nixos-auto-update
b. it's important to always use a single subagent for the monitoring, git pull, regenerate and fix.
