# Retired name-based identity shortcut

The prototype assumed visually similar labels were the same account and suggested
unioning grants when display names collided. It also stripped accents and mapped
similar-looking script characters to Latin. U4 supersedes these shortcuts.

Under the current rule table, ordinary Cafe and Café are different lookup keys.
An unsupported script character is rejected, not silently replaced. Preserve
original spelling and stable-ID history instead of rewriting it to disguise
ambiguous ownership.
