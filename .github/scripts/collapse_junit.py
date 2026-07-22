# Collapses the per-test-case testsuites produced by looping interoperability_report.py
# with -t into a single testsuite, matching the shape of a whole-suite run.

import sys
import xml.etree.ElementTree as ET


def collapse(path):
    tree = ET.parse(path)
    root = tree.getroot()
    suites = list(root)

    if len(suites) <= 1:
        return

    cases = []
    total_time = 0.0
    for suite in suites:
        total_time += float(suite.get("time") or 0)
        cases.extend(list(suite))
        root.remove(suite)

    merged = ET.SubElement(root, "testsuite", dict(suites[0].attrib))
    merged.extend(cases)

    counts = {
        "tests": len(cases),
        "failures": sum(1 for c in cases if c.find("failure") is not None),
        "errors": sum(1 for c in cases if c.find("error") is not None),
        "skipped": sum(1 for c in cases if c.find("skipped") is not None),
    }
    for key, value in counts.items():
        merged.set(key, str(value))
        root.set(key, str(value))
    merged.set("time", "%f" % total_time)
    root.set("time", "%f" % total_time)

    tree.write(path, encoding="utf-8", xml_declaration=True)


if __name__ == "__main__":
    collapse(sys.argv[1])
