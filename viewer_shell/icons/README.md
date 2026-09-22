Copy the app's existing icon here as `wraithguard_toolkit_icon.ico` before
building (CI does this as a build step -- see the workflow changes). Kept
out of this directory in source control since it's just a copy of
`wraithguard_toolkit_icon.ico` from the repo root; duplicating a binary
asset across two locations invites the two copies drifting.
