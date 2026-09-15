# Retired recovery card

The early single-replica prototype closed the guard on any successful response
and allowed each process one probe. It also treated green API health after the
cooldown as proof of dependency recovery. G5 supersedes those assumptions for
this shared dependency boundary.

Do not count client validation or permission failures toward the dependency
threshold. A failure to satisfy local policy is not evidence the renderer is
unavailable. Retain this card only to explain older operational behavior.
