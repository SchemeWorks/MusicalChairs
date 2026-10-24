#!/usr/bin/env python3
"""Repair a `graphify update` result against the accepted (backup) graph, then merge one semantic fragment.

    python3 -I graph_repair_merge.py <backup.json> <updated.json> <fragment.json> <out.json> \
        --mode validate|apply [--repo DIR] [--report FILE] [--max-restored-nodes N]

Standard library only.  The graph is a networkx node-link JSON (`directed: false`, `multigraph: false`), so a
`graphify update` that round-trips it through a simple networkx Graph collapses RAW PARALLEL EDGES: links that
share an unordered {source,target} pair survive only once.  It also rewrites hand-curated semantic nodes from
AST output (dropping `status`, `rationale`, ... and overwriting `label`/`file_type`).  This script:

 1. REPAIR (backup -> updated; the updated graph is authoritative for code nodes, the backup for curated ones)
    * nodes: every backup node id must exist.  A missing one is restored, EXCEPT a pure code node whose
      source_file no longer exists in the repo at HEAD and which no committed fragment defines (a legitimate
      removal; listed, not restored).  If more than --max-restored-nodes would need restoring the run STOPS.
    * node attributes: any key present in the backup but absent in the updated node is restored.  For CURATED
      nodes (id defined by a committed fragment under docs/solana/evidence or docs/coordination/evidence, and
      not a code node in the backup) the backup value also wins where the updated value differs, except
      `community` (recomputed by the clustering pass).  Changed attributes of every other node are untouched.
    * raw edges: for each unordered endpoint pair the updated link count must be >= the backup count; missing
      copies are restored from the backup with all attributes and the backup's source/target direction.
    * hyperedges: every backup hyperedge id must exist; a missing one is restored.
 2. MERGE the fragment: nodes (duplicate id = error), edges (missing endpoint = error), hyperedges (duplicate id
    or missing member = error) and `attribute_updates` (missing node = error).
 3. INVARIANTS (printed as JSON; `apply` writes <out.json> only if ALL gating invariants pass).

Exit status: 0 all invariants hold, 1 an invariant failed, 2 aborted (bad input / too many missing nodes).
"""
import argparse
import collections
import copy
import glob
import hashlib
import json
import os
import sys

EVIDENCE_GLOBS = ("docs/solana/evidence/*.json", "docs/coordination/evidence/*.json")
NODE_REQUIRED = ("id", "label", "file_type", "source_file", "source_location", "community")


class Abort(Exception):
    pass


def load_json(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def pair_of(link):
    return frozenset((link["source"], link["target"]))


def link_sig(link):
    return (link["source"], link["target"], link.get("relation"))


def canon(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False)


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


# --------------------------------------------------------------------------------------------------------------
# committed fragments
# --------------------------------------------------------------------------------------------------------------
def committed_fragments(repo, skip_realpath):
    """Every committed evidence JSON that carries semantic nodes (a `nodes` list of objects with an `id`)."""
    found = {}
    for pattern in EVIDENCE_GLOBS:
        for path in sorted(glob.glob(os.path.join(repo, pattern))):
            if os.path.realpath(path) == skip_realpath:
                continue
            try:
                data = load_json(path)
            except (OSError, ValueError):
                continue
            nodes = data.get("nodes") if isinstance(data, dict) else None
            if isinstance(nodes, list) and nodes and all(isinstance(n, dict) and "id" in n for n in nodes):
                found[os.path.relpath(path, repo)] = data
    return found


# --------------------------------------------------------------------------------------------------------------
# repair
# --------------------------------------------------------------------------------------------------------------
def repair(backup, graph, curated_ids, repo, max_restored_nodes):
    """Mutates `graph` (a deep copy of the updated graph).  Returns the repair record."""
    rec = {}
    backup_nodes = {n["id"]: n for n in backup["nodes"]}
    graph_nodes = {n["id"]: n for n in graph["nodes"]}

    # ---- nodes ---------------------------------------------------------------------------------------------
    missing = [i for i in backup_nodes if i not in graph_nodes]
    legit, to_restore = [], []
    for node_id in missing:
        node = backup_nodes[node_id]
        source_file = node.get("source_file")
        gone = bool(source_file) and not os.path.exists(os.path.join(repo, source_file))
        if node.get("file_type") == "code" and gone and node_id not in curated_ids:
            legit.append(node_id)
        else:
            to_restore.append(node_id)
    rec["backup_nodes_missing_in_updated"] = len(missing)
    rec["legitimate_code_removals"] = [
        {"id": i, "source_file": backup_nodes[i].get("source_file"), "label": backup_nodes[i].get("label")} for i in legit
    ]
    if len(to_restore) > max_restored_nodes:
        raise Abort(
            "STOP: %d backup nodes are missing from the updated graph and are not legitimate code removals "
            "(limit %d); not restoring wholesale. First ids: %s" % (len(to_restore), max_restored_nodes, to_restore[:10])
        )
    for node_id in to_restore:
        graph["nodes"].append(copy.deepcopy(backup_nodes[node_id]))
        graph_nodes[node_id] = graph["nodes"][-1]
    rec["restored_nodes"] = list(to_restore)

    # ---- node attributes -----------------------------------------------------------------------------------
    missing_key_counts = collections.Counter()
    override_counts = collections.Counter()
    changed_not_restored = collections.Counter()
    touched = []
    for node_id, old in backup_nodes.items():
        if node_id in legit or node_id in to_restore:
            continue
        new = graph_nodes[node_id]
        curated = node_id in curated_ids and old.get("file_type") != "code"
        hit = False
        for key, value in old.items():
            if key not in new:
                new[key] = copy.deepcopy(value)
                missing_key_counts[key] += 1
                hit = True
            elif curated and key != "community" and new[key] != value:
                new[key] = copy.deepcopy(value)
                override_counts[key] += 1
                hit = True
            elif not curated and new[key] != value:
                changed_not_restored[key] += 1
        if hit:
            touched.append(node_id)
    combined = collections.Counter(missing_key_counts)
    combined.update(override_counts)
    rec["attributes_restored"] = dict(combined)
    rec["attributes_restored_missing_key"] = dict(missing_key_counts)
    rec["attributes_restored_curated_override"] = dict(override_counts)
    rec["attributes_changed_by_update_left_alone"] = dict(changed_not_restored)
    rec["nodes_with_attributes_restored"] = touched

    # ---- raw edge multiplicity (identity = unordered endpoint pair) ---------------------------------------
    removed = set(legit)
    graph_by_pair = collections.defaultdict(list)
    for link in graph["links"]:
        graph_by_pair[pair_of(link)].append(link)
    backup_by_pair = collections.OrderedDict()
    for link in backup["links"]:
        backup_by_pair.setdefault(pair_of(link), []).append(link)
    restored_edges = []
    for pair, b_links in backup_by_pair.items():
        if pair & removed:
            continue
        u_links = list(graph_by_pair.get(pair, []))
        if len(u_links) >= len(b_links):
            continue
        # Pair every updated link with the backup link it most plausibly is (identical dict first, then the same
        # endpoints/relation/location, then endpoints/relation, then relation in either direction); the backup
        # links left unpaired beyond the count of updated links that remain are the collapsed raw copies.
        remaining_u = list(u_links)
        unmatched = list(b_links)
        matchers = (
            lambda u, b: canon(u) == canon(b),
            lambda u, b: link_sig(u) == link_sig(b)
            and (u.get("source_file"), u.get("source_location")) == (b.get("source_file"), b.get("source_location")),
            lambda u, b: link_sig(u) == link_sig(b),
            lambda u, b: u.get("relation") == b.get("relation"),
        )
        for matches in matchers:
            left = []
            for b_link in unmatched:
                hit = next((u for u in remaining_u if matches(u, b_link)), None)
                if hit is None:
                    left.append(b_link)
                else:
                    remaining_u.remove(hit)
            unmatched = left
        still = unmatched[len(remaining_u):]  # leftover updated links stand in for leftover backup links
        for b_link in still:
            restored_edges.append(copy.deepcopy(b_link))
    graph["links"].extend(restored_edges)
    rec["restored_raw_edges"] = restored_edges

    # ---- hyperedges ----------------------------------------------------------------------------------------
    have = {h["id"] for h in graph["hyperedges"]}
    restored_hyper = []
    for hyper in backup["hyperedges"]:
        if hyper["id"] not in have:
            graph["hyperedges"].append(copy.deepcopy(hyper))
            restored_hyper.append(hyper["id"])
    rec["restored_hyperedges"] = restored_hyper
    now = {h["id"]: h for h in graph["hyperedges"]}
    rec["hyperedges_changed_by_update"] = [h["id"] for h in backup["hyperedges"] if h["id"] in now and now[h["id"]] != h]
    return rec


# --------------------------------------------------------------------------------------------------------------
# fragment merge
# --------------------------------------------------------------------------------------------------------------
def merge_fragment(graph, fragment):
    errors = []
    ids = {n["id"] for n in graph["nodes"]}
    new_nodes = fragment.get("nodes", [])
    seen = set()
    for node in new_nodes:
        for key in NODE_REQUIRED:
            if key not in node:
                errors.append("fragment node %r lacks %r" % (node.get("id"), key))
        if node["id"] in ids or node["id"] in seen:
            errors.append("duplicate node id %r" % node["id"])
        seen.add(node["id"])
    if errors:
        raise Abort("fragment nodes invalid: " + "; ".join(errors))
    graph["nodes"].extend(copy.deepcopy(new_nodes))
    ids |= seen

    existing_pairs = collections.Counter(pair_of(l) for l in graph["links"])
    new_pair_counts = collections.Counter()
    shared_pairs = []
    for edge in fragment.get("edges", []):
        for key in ("source", "target", "relation", "confidence", "confidence_score", "source_file", "source_location"):
            if key not in edge:
                errors.append("fragment edge %r lacks %r" % ((edge.get("source"), edge.get("target")), key))
        for end in ("source", "target"):
            if edge.get(end) not in ids:
                errors.append("fragment edge endpoint missing from graph: %r" % edge.get(end))
        if edge.get("source") == edge.get("target"):
            errors.append("fragment self-loop on %r" % edge.get("source"))
        p = pair_of(edge) if "source" in edge and "target" in edge else None
        if p is not None:
            if existing_pairs.get(p) or new_pair_counts.get(p):
                shared_pairs.append(sorted(p))
            new_pair_counts[p] += 1
    if errors:
        raise Abort("fragment edges invalid: " + "; ".join(errors))
    graph["links"].extend(copy.deepcopy(fragment.get("edges", [])))

    hyper_ids = {h["id"] for h in graph["hyperedges"]}
    for hyper in fragment.get("hyperedges", []):
        if hyper["id"] in hyper_ids:
            errors.append("duplicate hyperedge id %r" % hyper["id"])
        hyper_ids.add(hyper["id"])
        for member in hyper["nodes"]:
            if member not in ids:
                errors.append("hyperedge %r member missing: %r" % (hyper["id"], member))
    if errors:
        raise Abort("fragment hyperedges invalid: " + "; ".join(errors))
    graph["hyperedges"].extend(copy.deepcopy(fragment.get("hyperedges", [])))

    by_id = {n["id"]: n for n in graph["nodes"]}
    applied = {}
    for node_id, changes in (fragment.get("attribute_updates") or {}).items():
        if node_id not in by_id:
            errors.append("attribute_updates target missing: %r" % node_id)
            continue
        by_id[node_id].update(copy.deepcopy(changes))
        applied[node_id] = sorted(changes)
    if errors:
        raise Abort("; ".join(errors))
    return {
        "nodes": len(new_nodes),
        "edges": len(fragment.get("edges", [])),
        "hyperedges": len(fragment.get("hyperedges", [])),
        "attribute_updates": len(applied),
        "attribute_update_keys": applied,
        "edges_sharing_an_endpoint_pair_with_another_link": shared_pairs,
        "community": (fragment.get("metadata") or {}).get("community"),
    }


# --------------------------------------------------------------------------------------------------------------
# invariants
# --------------------------------------------------------------------------------------------------------------
def invariants(backup, updated, graph, fragment, committed, repair_rec, merge_rec):
    inv = {}
    gate = {}
    final_ids = [n["id"] for n in graph["nodes"]]
    final_set = set(final_ids)
    backup_ids = {n["id"] for n in backup["nodes"]}
    legit = {r["id"] for r in repair_rec["legitimate_code_removals"]}

    gate["every_backup_node_id_preserved_or_listed_legitimate_removal"] = all(
        i in final_set or i in legit for i in backup_ids
    )
    inv["legitimate_code_removals"] = sorted(legit)
    inv["backup_node_ids_restored"] = len(repair_rec["restored_nodes"])

    # raw multiplicity (unordered pair) vs the backup, ignoring pairs that touch a legitimate removal
    final_pairs = collections.Counter(pair_of(l) for l in graph["links"])
    backup_pairs = collections.Counter(pair_of(l) for l in backup["links"])
    shortfalls = [sorted(p) for p, c in backup_pairs.items() if not (p & legit) and final_pairs.get(p, 0) < c]
    gate["backup_raw_edge_multiplicity_preserved"] = not shortfalls
    inv["raw_edge_multiplicity_shortfalls"] = len(shortfalls)
    # informational: same check at (pair, relation) granularity
    final_rel = collections.Counter((pair_of(l), l.get("relation")) for l in graph["links"])
    backup_rel = collections.Counter((pair_of(l), l.get("relation")) for l in backup["links"])
    inv["relation_level_shortfalls_informational"] = sum(
        1 for k, c in backup_rel.items() if not (k[0] & legit) and final_rel.get(k, 0) < c
    )

    final_hyper = {h["id"]: h for h in graph["hyperedges"]}
    gate["backup_hyperedges_preserved"] = all(h["id"] in final_hyper for h in backup["hyperedges"])
    gate["hyperedge_lists_in_sync_top_level_and_graph"] = graph["hyperedges"] == graph.get("graph", {}).get("hyperedges", graph["hyperedges"])

    gate["duplicate_node_ids_0"] = len(final_ids) == len(final_set)
    dangling = [l for l in graph["links"] if l["source"] not in final_set or l["target"] not in final_set]
    dangling_h = [(h["id"], m) for h in graph["hyperedges"] for m in h["nodes"] if m not in final_set]
    gate["dangling_edges_0"] = not dangling
    gate["dangling_hyperedge_members_0"] = not dangling_h
    inv["dangling_edges"] = len(dangling)
    inv["dangling_hyperedge_members"] = len(dangling_h)

    # edge direction: every restored edge is present exactly as in the backup; informational flips elsewhere
    final_canon = collections.Counter(canon(l) for l in graph["links"])
    gate["restored_edges_keep_backup_direction_and_attributes"] = all(
        final_canon.get(canon(e), 0) >= 1 for e in repair_rec["restored_raw_edges"]
    )
    backup_canon = collections.Counter(canon(l) for l in backup["links"])
    not_verbatim = [k for k, c in backup_canon.items() if final_canon.get(k, 0) < c]
    inv["backup_links_not_verbatim_in_final_informational"] = len(not_verbatim)
    committed_edge_canon = {canon(e) for d in committed.values() for e in d.get("edges", [])}
    gate["no_committed_fragment_edge_altered_vs_backup"] = not any(k in committed_edge_canon for k in not_verbatim)
    final_by_pair = collections.defaultdict(list)
    for link in graph["links"]:
        final_by_pair[pair_of(link)].append(link)
    flips = 0
    for link in backup["links"]:
        if pair_of(link) & legit:
            continue
        group = final_by_pair.get(pair_of(link), [])
        if group and not any((g["source"], g["target"]) == (link["source"], link["target"]) for g in group):
            flips += 1
    inv["backup_links_whose_direction_differs_from_every_final_link_on_the_pair_informational"] = flips
    gate["fragment_edges_keep_fragment_direction"] = all(
        canon(e) in final_canon for e in fragment.get("edges", [])
    )

    # committed fragments: every node, hyperedge and edge pair present; curated attributes intact
    nodes_by_id = {n["id"]: n for n in graph["nodes"]}
    backup_by_id = {n["id"]: n for n in backup["nodes"]}
    committed_ids = {}
    for path, data in committed.items():
        for node in data["nodes"]:
            committed_ids[node["id"]] = path
    missing_committed = sorted(i for i in committed_ids if i not in final_set)
    gate["all_committed_fragment_nodes_present"] = not missing_committed
    inv["committed_fragment_files"] = len(committed)
    inv["committed_fragment_node_ids"] = len(committed_ids)
    inv["committed_fragment_nodes_missing"] = missing_committed[:10]
    missing_h = sorted(h["id"] for d in committed.values() for h in d.get("hyperedges", []) if h["id"] not in final_hyper)
    gate["all_committed_fragment_hyperedges_present"] = not missing_h
    # Edges of committed fragments that the ACCEPTED (backup) graph already lacked are earlier-superseded edges
    # (the 2026-10-12 fragments were replaced by the later planning fragments); they are reported, not gated.
    missing_e = [
        (e["source"], e["target"]) for d in committed.values() for e in d.get("edges", []) if pair_of(e) not in final_pairs
    ]
    lost_e = [pr for pr in missing_e if frozenset(pr) in backup_pairs]
    gate["all_committed_fragment_edge_pairs_present_that_the_backup_had"] = not lost_e
    inv["committed_fragment_hyperedges_missing"] = missing_h[:10]
    inv["committed_fragment_edge_pairs_missing_in_final_and_already_absent_from_backup_informational"] = len(missing_e) - len(lost_e)
    inv["committed_fragment_edge_pairs_lost_vs_backup"] = len(lost_e)
    stripped = 0
    for node_id in committed_ids:
        old = backup_by_id.get(node_id)
        new = nodes_by_id.get(node_id)
        if old is None or new is None or old.get("file_type") == "code":
            continue
        for key, value in old.items():
            if key == "community" or key in (fragment.get("attribute_updates") or {}).get(node_id, {}):
                continue
            if new.get(key, "<absent>") != value:
                stripped += 1
    gate["curated_semantic_node_attributes_identical_to_backup"] = stripped == 0
    inv["curated_attribute_mismatches"] = stripped
    code_status_lost = sum(
        1 for i, old in backup_by_id.items()
        if "status" in old and i in nodes_by_id and "status" not in nodes_by_id[i]
    )
    gate["no_node_that_had_a_status_lost_it"] = code_status_lost == 0

    # the fragment itself
    frag_ids = [n["id"] for n in fragment.get("nodes", [])]
    gate["fragment_nodes_all_present"] = all(i in final_set for i in frag_ids)
    gate["fragment_node_ids_unique_vs_backup"] = not any(i in backup_ids for i in frag_ids)
    community = (fragment.get("metadata") or {}).get("community")
    used = {n.get("community") for n in graph["nodes"] if n["id"] not in set(frag_ids)}
    gate["fragment_community_unused_by_other_nodes"] = community is not None and community not in used and all(
        n.get("community") == community for n in fragment.get("nodes", [])
    )
    inv["max_community_among_non_fragment_nodes"] = max(c for c in used if isinstance(c, int))
    gate["fragment_attribute_updates_applied"] = all(
        nodes_by_id[i].get(k) == v
        for i, changes in (fragment.get("attribute_updates") or {}).items()
        for k, v in changes.items()
    )
    gate["graph_flags_unchanged"] = (graph["directed"], graph["multigraph"]) == (backup["directed"], backup["multigraph"])
    inv["gate"] = gate
    inv["all_pass"] = all(gate.values())
    return inv


# --------------------------------------------------------------------------------------------------------------
def main(argv):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("backup")
    parser.add_argument("updated")
    parser.add_argument("fragment")
    parser.add_argument("out")
    parser.add_argument("--mode", choices=("validate", "apply"), required=True)
    parser.add_argument("--repo", default=".", help="repository checkout at the commit the graph covers (default: cwd)")
    parser.add_argument("--report", default=None, help="repair record JSON (apply default: <out>.repairs.json)")
    parser.add_argument("--max-restored-nodes", type=int, default=25)
    args = parser.parse_args(argv)

    repo = os.path.abspath(args.repo)
    if not os.path.isdir(os.path.join(repo, "docs", "solana", "evidence")):
        print(json.dumps({"error": "--repo %s has no docs/solana/evidence" % repo}))
        return 2
    try:
        backup = load_json(args.backup)
        updated = load_json(args.updated)
        fragment = load_json(args.fragment)
        committed = committed_fragments(repo, os.path.realpath(args.fragment))
        curated_ids = {n["id"] for d in committed.values() for n in d["nodes"]}
        graph = copy.deepcopy(updated)
        graph.setdefault("graph", {})
        repair_rec = repair(backup, graph, curated_ids, repo, args.max_restored_nodes)
        merge_rec = merge_fragment(graph, fragment)
        graph["graph"]["hyperedges"] = graph["hyperedges"]
        inv = invariants(backup, updated, graph, fragment, committed, repair_rec, merge_rec)
    except Abort as error:
        print(json.dumps({"mode": args.mode, "aborted": str(error)}, indent=1))
        return 2

    summary = {
        "mode": args.mode,
        "repo": repo,
        "inputs": {"backup": [len(backup["nodes"]), len(backup["links"]), len(backup["hyperedges"])],
                   "updated": [len(updated["nodes"]), len(updated["links"]), len(updated["hyperedges"])]},
        "repairs": {
            "restored_nodes": len(repair_rec["restored_nodes"]),
            "backup_nodes_missing_in_updated": repair_rec["backup_nodes_missing_in_updated"],
            "legitimate_code_removals": repair_rec["legitimate_code_removals"],
            "nodes_with_attributes_restored": len(repair_rec["nodes_with_attributes_restored"]),
            "attributes_restored": repair_rec["attributes_restored"],
            "attributes_restored_missing_key": repair_rec["attributes_restored_missing_key"],
            "attributes_restored_curated_override": repair_rec["attributes_restored_curated_override"],
            "attributes_changed_by_update_left_alone": repair_rec["attributes_changed_by_update_left_alone"],
            "restored_raw_edges": len(repair_rec["restored_raw_edges"]),
            "restored_raw_edge_pairs": [
                [e["source"], e["target"], e.get("relation")] for e in repair_rec["restored_raw_edges"]
            ],
            "restored_hyperedges": repair_rec["restored_hyperedges"],
            "hyperedges_changed_by_update": repair_rec["hyperedges_changed_by_update"],
        },
        "fragment": {k: v for k, v in merge_rec.items() if k != "attribute_update_keys"}
        | {"attribute_update_keys": merge_rec["attribute_update_keys"]},
        "invariants": inv,
        "final_counts": {"nodes": len(graph["nodes"]), "edges": len(graph["links"]), "hyperedges": len(graph["hyperedges"])},
    }

    if args.mode == "apply" and inv["all_pass"]:
        with open(args.out, "w", encoding="utf-8") as handle:
            json.dump(graph, handle, ensure_ascii=False, separators=(",", ":"))
        summary["out"] = {"path": os.path.abspath(args.out), "sha256": sha256_file(args.out)}
        report_path = args.report or args.out + ".repairs.json"
        with open(report_path, "w", encoding="utf-8") as handle:
            json.dump(
                {
                    "restored_raw_edges": repair_rec["restored_raw_edges"],
                    "attributes_restored": repair_rec["attributes_restored"],
                    "nodes_with_attributes_restored": repair_rec["nodes_with_attributes_restored"],
                    "same_symbol_as_edges": [],
                },
                handle, ensure_ascii=False, indent=1,
            )
        summary["repairs_report"] = os.path.abspath(report_path)
    elif args.mode == "apply":
        summary["out"] = "NOT WRITTEN: an invariant failed"
    print(json.dumps(summary, indent=1, ensure_ascii=False))
    return 0 if inv["all_pass"] else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
