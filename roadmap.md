# mdlr Roadmap

Add extractors for additional languages.

- [ ] Go extractor (`.go`)
- [ ] Python extractor (`.py`)

Metrics that capture higher-level modularity concerns.

- [ ] `concept_scatter` - How spread out is a concept across the codebase?
- [ ] `closure` - What's the transitive closure of dependencies?
- [ ] `edge_cut_ratio` - How cleanly can the graph be partitioned?

Better detection of data flow relationships.

- [ ] Reads analysis - Track which entities a unit consumes
- [ ] Writes analysis - Track which entities a unit produces
- [ ] Data flow edges - Connect producers to consumers

General ideas

- [ ] Duplicated code fragments
