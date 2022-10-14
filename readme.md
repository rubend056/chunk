# Chunk
To [chunk information](https://en.wikipedia.org/wiki/Chunking_%28psychology%29?wprov=sfla1), a process by which individual pieces of an information set are bound together into a meaningful whole.

**Basically adding relationships/views to markdown**

## Goals
- **Redability** (information is easy to zoom, pick apart, and digest, it should feel like an interactable Kurtzkezart video)
- **Editability** (editing is a click/tap away)
- *Replicability* (your devices should be able to cache this data for offline use, then synchronize at a later date)

It's easy to see the crucial thing is **Chunk Data Visualization** built on top of a solid **Chunk Relationship Logic**.

---

## Overview
- Dead simple text markup
- Data relationships explicit in syntax
- Different visualization options

### Implemetation
- CommonMark + Custom Data Relation Syntax
- Different visualization options
  - Editing
    - ![](web/src/assets/icons/card-text.svg) **Shank/Edit** -> selected chunk + children up to 4N (1N default) an editor
  - Viewing
    - ![](web/src/assets/icons/clipboard.svg) **Notes** -> chunks ordered by recent side by side
    - ![](web/src/assets/icons/grid.svg) **Labyrinth** -> selected chunk children on a grid
    - ![](web/src/assets/icons/diagram-2-fill.svg) **Graph** -> nodes in a tree

## Definitions

```
Chunk {
	value: string
	created: utc seconds since epoch
	modified: utc seconds since epoch
}
User {
	user: string
	pass: string
	salt: string (for brute force attacks)
}
```

## Chunk Logic
A mockup of how chunks should be displayed under each view. This should give us an idea of how complex the system will be.

A chunk's <u>header</u> is defined [by regex](https://regexr.com/6vm4s) 
`^#  *(?<title>(?: *[\w]+)+) *(?:[-=]> *(?<relations>(?:,? *[\w]+)+) *)?$` which extracts **title** and **relations**. 

What advantage does relating chunks give me? Well that's the whole point, chunks relating to other chunks, but instead of putting it all in a big long list, this UI will nudge the user towards keeping their children list small 4-6 (green), 7-8 (yellow), 9+ (red). Yes, colors are important.

### Notes

| `# Chores` |`# Groceries -> Chores`|
-|-
| `# NYCTrip -> Groceries` |`# Friday -> Groceries`|

```
# Chores

Stuff I seriously don't like doing but I have to do anyways for my own well being and sanity.

## Groceries

## House Stuff
- Clean
- Do dishes
```
What would be the representation of the different views?

## Implementations

### MVP0 (Complete)
Basically a notes app
- Chunk Logic
- Views: Notes & Edit with 0 Children

### MVP1 (Complete)
Create release of project

### MVP2
