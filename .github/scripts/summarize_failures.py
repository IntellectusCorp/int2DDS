# Turns the merged JUnit report into two plain-text artifacts:
#   failures.txt  - one "suite::case" line per failed case, sorted (diffable against a baseline)
#   summary.txt   - suite, case, expected -> produced codes, and the run arguments
# The failure detail is an HTML table stored in the <failure message="..."> attribute.

import re
import sys
import xml.etree.ElementTree as ET

CELL = re.compile(r"<th>\s*(.*?)\s*</th>", re.DOTALL)


def code_rows(message):
    # The first <table> holds rows of: role | expected code | produced code.
    table = message.split("</table>")[0]
    rows = []
    for row in re.findall(r"<tr>(.*?)</tr>", table, re.DOTALL):
        cells = [c.strip() for c in CELL.findall(row)]
        if len(cells) == 3 and cells[1] != "Expected Code":
            rows.append(tuple(cells))
    return rows


def collect(report_path):
    root = ET.parse(report_path).getroot()
    failures = []
    for suite in root:
        suite_name = suite.get("name") or "?"
        for case in suite:
            failure = case.find("failure")
            if failure is None:
                continue
            failures.append((suite_name, case.get("name") or "?", case, failure))
    return failures


def main(report_path, failures_out, summary_out):
    failures = collect(report_path)

    with open(failures_out, "w", encoding="utf-8") as f:
        for suite, case, _, _ in sorted(failures):
            f.write("%s::%s\n" % (suite, case))

    with open(summary_out, "w", encoding="utf-8") as f:
        f.write("%d failed cases\n\n" % len(failures))
        for suite, case, case_el, failure in sorted(failures):
            f.write("%s :: %s\n" % (suite, case))

            for role, expected, produced in code_rows(failure.get("message") or ""):
                mark = "  " if expected == produced else "!!"
                f.write("  %s %-12s expected %-24s produced %s\n"
                        % (mark, role, expected, produced))

            for key, value in case_el.attrib.items():
                if "ublisher" in key or "ubscriber" in key:
                    f.write("     %s = %s\n" % (key, value))
            f.write("\n")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])
