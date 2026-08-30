# Routing globe preview master experiment

This checkpoint starts from Natural Earth 1:50m land polygons and rebuilds only the flat Preview master.

- The source outline view uses all non-Antarctic 1:50m land rings.
- The Preview master uses a light 0.045 degree Douglas-Peucker cleanup.
- Only very small noise rings are removed. East/Southeast Asia, the Mediterranean, the British Isles, and New Zealand use a lower small-island threshold.
- Taiwan, Japan, the Philippines, the United Kingdom, and New Zealand are asserted as present.
- No 24px/32px icon master, orthographic frame, animation, or route UI is generated here.
