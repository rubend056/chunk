# Chunk
To [chunk information](https://en.wikipedia.org/wiki/Chunking_%28psychology%29?wprov=sfla1), a process by which individual pieces of an information set are bound together into a meaningful whole.



## What we need:
- Easily think through/chunk things together.
- Dead simple text markup
- Data relationships are explicit
- Different visualization options
    - Graph
- (Maybe) Only 1 full screen where we pan/zoom/select/type.

Chunk `id = title` (Lowercased, trimmed, replacing space by underscore and removing all [^a-z_0-9]). This allows pretty formatting of titles but standardizes the ids.



## Basics
- Definitions of Objects
    - Chunk
        - value: string
        - created: utc seconds since epoch
        - modified: utc seconds since epoch
    - User
        - user: string
        - pass: string
        - salt: string (for brute force attacks)



## Implementation
