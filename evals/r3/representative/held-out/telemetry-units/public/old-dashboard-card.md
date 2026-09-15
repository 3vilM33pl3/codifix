# Retired dashboard-only unit assumption

Before mixed firmware, every device emitted Celsius, so the old card recommended
storing the number unchanged and placing the unit in the dashboard label. It also
treated large jumps at upgrade time as chart artifacts to mute temporarily.

T6 supersedes that assumption. Mixed source units require conversion at ingestion
and retained raw provenance. A chart-only suffix edit cannot correct canonical
storage, historical data or threshold evaluation. Retain the old card to explain
the c31 behavior, not as authority to silence all warnings.
