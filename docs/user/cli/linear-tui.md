# Manual Linear issue review

Run the installed `codifix-linear` launcher to draft, correct, and approve a
Linear issue using the resident companion. The launcher supplies the local
operator profile to the internal `codifix linear-tui --profile …` entry point.

1. Open `codifix-linear` with the approved project binding. Enter a title and description. Use **Tab** to move between fields and **Ctrl+S** to send the draft for review.
2. Read the proposed issue and wording. Use **Ctrl+E** to correct the draft and **Ctrl+S** to submit the revision on the same task. A revision receives its own confirmation digest.
3. When the proposal matches what you intend to publish, press **Enter** in the confirmation field to approve the displayed wording. The companion applies it and reads the saved issue back.
4. Open the returned Linear URL and keep it with your task evidence. If approval was sent but its acknowledgement was lost, **Enter** retries that same response.

For automatic tracking from approved checkpoints, see [Linear project tracking](https://codifix.dev/docs/specialists/linear).
