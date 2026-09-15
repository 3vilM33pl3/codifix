#!/usr/bin/env python3
"""Build independent arithmetic fixtures for the approved M2 employee-AI revision.

This is a test-data builder, not a second forecast. Expected amounts use bounded
fictional cases and explicit headcount-month arithmetic, not workbook formulas.
The original thirteen fixtures and v1 native evidence remain unchanged. Their
v2 regression variants add only the historical staffing selector and zero AI
allowance, then run through the same native engine and verified refresh.
"""
from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "validation-cases.json"
LEGACY_HIRES = [1, 1, 2, 3, 2, 5, 6, 8, 10, 12, 15, 16, 18, 19, 21, 23, 26, 28]
LEAN_HIRES = {5: 1, 6: 1, 7: 2, 8: 4, 12: 8, 10: 10, 13: 12, 16: 16}


def checks(items):
    return [dict(cell=cell, expected=value, tolerance=0.00001, label=label)
            for cell, value, label in items]


def lean_profile():
    result = {"Assumptions!B118": 0}
    for row, legacy_hire in zip(range(5, 23), LEGACY_HIRES):
        result[f"Team!AQ{row}"] = int(row in LEAN_HIRES)
        result[f"Team!AM{row}"] = LEAN_HIRES.get(row, legacy_hire)
    return result


def one_employee(row=5, hire=3):
    return {**{f"Team!AQ{r}": 0 for r in range(5, 23)},
            f"Team!AQ{row}": 1, f"Team!AM{row}": hire}


def build_cases():
    original = json.loads(SOURCE.read_text())
    base = copy.deepcopy(next(c["inputs"] for c in original if c["id"] == "funding_zero_sales"))
    # Intentionally retain paid headcount while removing unrelated company costs.
    # Salary multiplier zero is a fictional isolation device, not an operating plan.
    base.update({**lean_profile(), "Assumptions!B69": 0,
                 "Assumptions!B78": 0, "Assumptions!B119": 5000,
                 "Assumptions!B120": 0.5, "Assumptions!B121": 250})
    result = []

    def add(id_, label, overrides, expected, note):
        result.append(dict(id=id_, label=label, inputs={**base, **overrides},
                           checks=checks(expected), notes=note))

    add("ai_lean_headcount_and_funding", "Approved lean hiring dates: 146 and 242 employee-months", {}, [
        ("Team!C27", 2, "Two employees in month 1"),
        ("Team!D27", 3, "Platform hire starts in month 2"),
        ("Team!E27", 3, "Retrieval hire is not yet active in month 3"),
        ("Team!F27", 4, "Retrieval hire starts in month 4"),
        ("Team!J27", 5, "Solutions hire starts in month 8"),
        ("Team!L27", 6, "Enterprise hire starts in month 10"),
        ("Summary!C18", 7, "Seven staff at year 1 end"),
        ("Summary!D18", 8, "Eight staff at year 2 end"),
        ("Summary!E18", 8, "Eight staff at year 3 end"),
        ("Summary!B72", 730000, "146 employee-months times 5000 allowance"),
        ("Summary!B73", 365000, "First 24 months expected AI cash spending"),
        ("Summary!B74", 365000, "First 24 months unspent allowance remains cash headroom"),
        ("Summary!B75", 1210000, "242 employee-months times 5000 allowance"),
        ("Summary!B76", 605000, "Full 36-month expected AI spending"),
        ("Summary!B77", 605000, "Full 36-month unspent allowance disclosure"),
        ("Summary!B42", 365000, "Gross cash uses exclude earmarked AI headroom"),
        ("Summary!B25", 60000, "Months 25 to 27 operating buffer is 3 times 8 times 2500"),
        ("Summary!B22", 790000, "Net funding covers 365k spending plus 365k headroom plus 60k buffer"),
        ("Summary!B27", 425000, "Month 24 closing cash includes headroom and operating buffer"),
        ("Summary!B78", 60000, "Month 24 cash excluding AI headroom equals operating buffer"),
        ("Summary!B84", 425000, "Month 24 cash target includes reserve once"),
    ], "Fictional AI-only company with zero salary multiplier and other costs. Commercial targets are not used as productivity evidence.")

    add("ai_single_hire_with_payroll", "One month-3 hire incurs salary, tools, recruitment and AI once", {
        **one_employee(), "Team!B5": 120000, "Team!AN5": 0.4,
        "Assumptions!B62": 1, "Assumptions!B68": 1, "Assumptions!B70": 1,
        "Assumptions!B72": 0.1, "Assumptions!B73": 1,
        "Assumptions!B74": 100, "Assumptions!B75": 200,
    }, [
        ("Team!D27", 0, "No employee before month 3"),
        ("Team!E27", 1, "Employee starts in month 3"),
        ("Team!D24", 0, "No payroll before hire"),
        ("Team!E24", 10000, "Annual 120k salary divided across twelve months"),
        ("Team!E25", 4000, "Forty percent delivery salary"),
        ("Team!E28", 6000, "Remaining sixty percent operating salary"),
        ("Team!E31", 12000, "Recruitment charged once at hire"),
        ("Team!F31", 0, "No repeated recruitment in following month"),
        ("'Employee AI'!D17", 0, "No AI spending before hire"),
        ("'Employee AI'!E14", 5000, "One employee receives 5000 allowance"),
        ("'Employee AI'!E15", 250, "Subscription within expected spending"),
        ("'Employee AI'!E16", 2250, "Metered spending excludes subscription portion"),
        ("'Employee AI'!E17", 2500, "Total expected AI expense"),
        ("'Employee AI'!E18", 2500, "Unspent allowance is cash headroom"),
        ("'Employee AI'!E20", 1000, "Delivery AI uses the forty percent staff allocation"),
        ("'Employee AI'!E21", 1500, "Remaining founder AI cost is product expense"),
        ("'Employee AI'!E22", 0, "Founder produces no GTM allocation"),
        ("Costs!E34", 5000, "Salary 4000 and AI 1000 are charged once to cost of sales"),
        ("'P&L & Cash'!E12", -100, "Software tools start at hire"),
        ("'P&L & Cash'!E13", -200, "Workspace starts at hire"),
        ("'P&L & Cash'!E25", 24800, "Hire month cash: salary 10k, AI 2.5k, tools/workspace 300, recruitment 12k"),
        ("'P&L & Cash'!F25", 12800, "Next month repeats salary, AI and employee overheads only"),
        ("Summary!B72", 110000, "22 eligible employee-months of allowance"),
        ("Summary!B73", 55000, "22 eligible employee-months of expected spending"),
        ("Summary!B74", 55000, "22 eligible employee-months of unused allowance"),
        ("Summary!B42", 293600, "22 times 12800 plus one 12000 recruitment fee"),
        ("Summary!B25", 38400, "Three operating months of 12800"),
        ("Summary!B22", 387000, "Funding includes 293600 cash expense, 38400 buffer and 55000 headroom"),
        ("Summary!B27", 93400, "Month 24 cash preserves both buffer and headroom"),
    ], "All excluded employees retain their positive regional salary inputs; only the single included employee may trigger payroll or related costs.")

    for utilisation, spend24, reserve24, buffer, funding, metered in [
        (0, 0, 110000, 0, 110000, 0),
        (0.5, 55000, 55000, 7500, 117500, 2250),
        (1, 110000, 0, 15000, 125000, 4750),
    ]:
        add(f"ai_utilisation_{int(utilisation * 100)}", f"One month-3 employee at {utilisation:.0%} utilisation", {
            **one_employee(), "Assumptions!B120": utilisation,
        }, [
            ("'Employee AI'!E15", 250 if utilisation else 0, "Subscriptions cannot exceed expected spend"),
            ("'Employee AI'!E16", metered, "Metered expense is additional to subscription-included use"),
            ("'Employee AI'!E17", 5000 * utilisation, "Monthly spending follows selected utilisation"),
            ("'Employee AI'!E18", 5000 * (1 - utilisation), "Monthly unused allowance is not expensed"),
            ("Summary!B73", spend24, "22 employee-months expected spending"),
            ("Summary!B74", reserve24, "22 employee-months unused allowance"),
            ("Summary!B42", spend24, "Cash uses contain expenses only"),
            ("Summary!B25", buffer, "Operating buffer follows expected spending"),
            ("Summary!B22", funding, "Funding reconciles spending, buffer and earmarked cash"),
            ("Summary!B27", reserve24 + buffer, "Closing cash retains unspent allocation and operating buffer"),
        ], "Expected employee-months are 22 through month 24 and 34 through month 36. No costs or receipts other than employee AI.")

    for allowance, utilisation, spending, metered, unused, funding in [
        (2000, 0.5, 1000, 750, 1000, 51000),
        (10000, 0.5, 5000, 4750, 5000, 255000),
        (5000, 0.25, 1250, 1000, 3750, 123750),
    ]:
        add(f"ai_allowance_{allowance}_utilisation_{int(utilisation * 100)}",
            "Allowance and utilisation sensitivities preserve the expense versus headroom split", {
                **one_employee(hire=1), "Assumptions!B119": allowance,
                "Assumptions!B120": utilisation,
            }, [
                ("'Employee AI'!C15", 250, "Subscription ceiling stays within expected spending"),
                ("'Employee AI'!C16", metered, "Additional metered cost excludes subscriptions"),
                ("'Employee AI'!C17", spending, "Allowance times utilisation is total expense"),
                ("'Employee AI'!C18", unused, "Unused allocation remains available cash"),
                ("Summary!B73", spending * 24, "Twenty-four employee-months expected expense"),
                ("Summary!B74", unused * 24, "Twenty-four employee-months unused allocation"),
                ("Summary!B25", spending * 3, "Buffer uses the next three expected operating payments"),
                ("Summary!B22", funding, "Funding includes current spending, future operating buffer and reserve"),
            ], "One employee starts in month 1; salary and unrelated company costs are zero only for this arithmetic fixture.")

    add("ai_all_roles_excluded", "Excluded roles produce no employee-linked costs despite positive inputs", {
        **{f"Team!AQ{r}": 0 for r in range(5, 23)},
        **{f"Team!AN{r}": 0.5 for r in range(5, 23)},
        "Assumptions!B62": 1, "Assumptions!B68": 1,
        "Assumptions!B72": 0.3, "Assumptions!B73": 1,
        "Assumptions!B74": 123, "Assumptions!B75": 234,
    }, [
        ("Summary!C18", 0, "No year 1 headcount"),
        ("Summary!D18", 0, "No year 2 headcount"),
        ("Summary!E18", 0, "No year 3 headcount"),
        ("Team!C24", 0, "Excluded early hires have no payroll"),
        ("Team!AL24", 0, "Excluded later hires have no payroll"),
        ("Team!AL26", 0, "Excluded employees supply no delivery capacity"),
        ("Team!C31", 0, "No recruitment for excluded founders"),
        ("Team!AD31", 0, "No recruitment for excluded later staff"),
        ("'P&L & Cash'!AL12", 0, "Excluded staff generate no tools cost"),
        ("'P&L & Cash'!AL13", 0, "Excluded staff generate no workspace cost"),
        ("Summary!B72", 0, "No employee AI allowance through month 24"),
        ("Summary!B75", 0, "No employee AI allowance through month 36"),
        ("Summary!B42", 0, "All employee cash spending excluded"),
        ("Summary!B22", 0, "No funding requirement in zero-cost isolated company"),
    ], "Positive salaries, delivery shares, recruitment rate, tools and workspace prices test inclusion gates independently of zero-cost inputs.")

    add("ai_hiring_delay_once", "Two-month hiring delay postpones every employee AI amount once", {
        **one_employee(), "Assumptions!B69": 2,
    }, [
        ("Team!F27", 0, "Month-3 hire delayed beyond month 4"),
        ("Team!G27", 1, "Delayed hire begins in month 5"),
        ("'Employee AI'!F17", 0, "No spending before delayed start"),
        ("'Employee AI'!G17", 2500, "Spending begins in month 5"),
        ("Summary!B72", 100000, "20 employee-months allowance to month 24"),
        ("Summary!B73", 50000, "20 employee-months spending to month 24"),
        ("Summary!B74", 50000, "20 employee-months unused allowance"),
        ("Summary!B75", 160000, "32 employee-months allowance to month 36"),
        ("Summary!B22", 107500, "Funding covers 50k expense, 50k headroom and 7500 buffer"),
    ], "A hire month of 3 plus delay of 2 means start in month 5; no double application of delay in the employee AI schedule.")

    two_roles = {**one_employee(hire=1), "Team!AQ13": 1, "Team!AM13": 1,
                 "Team!AN5": 0.4, "Team!AN13": 0.2}
    for ownership, delivery, product, gtm in [(1, 1500, 1500, 2000), (0.5, 750, 2000, 2250)]:
        add(f"ai_allocation_owner_{int(ownership * 100)}", "Two employees allocate AI by function and delivery ownership", {
            **two_roles, "Assumptions!B62": ownership,
        }, [
            ("'Employee AI'!C17", 5000, "Total employee AI cost is unchanged by allocation"),
            ("'Employee AI'!C20", delivery, "Delivery shares times Codifix delivery ownership"),
            ("'Employee AI'!C21", product, "Remaining founder AI spend is Product"),
            ("'Employee AI'!C22", gtm, "Remaining commercial AI spend is GTM"),
            ("'Employee AI'!C23", 5000, "Three allocations exhaust total employee AI spending"),
            ("'Employee AI'!C24", 0, "No double allocation residual"),
            ("Costs!C34", delivery, "Cost of sales receives allocated employee AI once"),
            ("'P&L & Cash'!C25", 5000, "Ownership changes classification without duplicating or dropping employee cost"),
        ], "Both employees have zero salary only to isolate AI. The founder is Product and the commercial row is GTM; all customer workload is zero.")

    add("ai_legacy_staffing_same_policy", "Historical 18-role plan receives the identical employee AI policy", {
        "Assumptions!B118": 1,
    }, [
        ("Summary!C18", 10, "Legacy first year headcount"),
        ("Summary!D18", 16, "Legacy second year headcount"),
        ("Summary!E18", 18, "Legacy third year headcount"),
        ("Summary!B72", 1190000, "238 legacy employee-months times 5000"),
        ("Summary!B73", 595000, "238 legacy employee-months times 2500"),
        ("Summary!B74", 595000, "Legacy first 24-month unused allowance"),
        ("Summary!B75", 2250000, "450 legacy employee-months times 5000"),
        ("Summary!B76", 1125000, "450 legacy employee-months times 2500"),
        ("Summary!B25", 125000, "Legacy following-quarter headcount of 16,17,17 at 2500 each"),
        ("Summary!B22", 1315000, "Legacy AI-only funding includes spending, reserve and operating buffer"),
        ("Summary!B79", 0, "Historical profile has no payroll saving against itself"),
    ], "Legacy source hire months are 1,1,2,3,2,5,6,8,10,12,15,16,18,19,21,23,26,28. This is a cost comparison, not proof of output equivalence.")

    add("ai_staffing_payroll_savings", "Uniform fictional salaries expose employee-month savings independently of regional pay", {
        **{f"Team!B{r}": 120000 for r in range(5, 23)},
        "Assumptions!B68": 1, "Assumptions!B70": 1,
    }, [
        ("Summary!B79", 920000, "92 fewer employee-months through month 24 at 10000 salary per month"),
        ("Summary!B80", 2080000, "208 fewer employee-months through month 36 at 10000 salary per month"),
        ("Summary!B42", 1825000, "146 employee-months salary plus 365000 expected AI spending"),
        ("Summary!B25", 300000, "24 employee-months of 10000 salary plus 2500 AI"),
        ("Summary!B22", 2490000, "Funding covers 1825000 spending, 300000 buffer and 365000 headroom"),
    ], "All eighteen salary cells are explicitly set to 120000, with load 1, so the saving derives only from timing and inclusion. AI headroom is not a spending reduction.")

    for support_included, capacity, shortfall, contractor, ai_delivery in [
        (1, 192, 18, 1350, 4000), (0, 72, 138, 10350, 1500),
    ]:
        add(f"ai_delivery_capacity_support_{support_included}", "Capacity shortfall purchases contractors without assuming AI throughput gains", {
            **one_employee(row=12, hire=1), "Team!AQ16": support_included,
            "Team!AM16": 1, "Team!AN12": 0.6, "Team!AN16": 1,
            "Assumptions!B62": 1, "Assumptions!B67": 20,
            "Assumptions!B19": 500, "Assumptions!B60": 75,
            "Assumptions!B61": 120, "Assumptions!B96": 1,
        }, [
            ("Revenue!I10", 1, "One paid account in month 7"),
            ("Costs!I28", 210, "Service effort 10.5 hours multiplied by stress factor 20"),
            ("Costs!I29", capacity, "Salaried capacity uses only included staff allocation"),
            ("Costs!I31", shortfall, "Contractor hours fill only the demand minus capacity gap"),
            ("Costs!I32", contractor, "Shortfall hours cost 75 each"),
            ("'Employee AI'!I20", ai_delivery, "Included employees alone receive delivery AI allocation"),
            ("'Employee AI'!I17", 5000 if support_included else 2500, "AI spend follows included employees without changing productivity"),
        ], "The single pilot starts month 5 and converts month 7. Demand and employee capacity remain separate from token allowance; removing support lowers capacity and raises contractor cost.")

    add("ai_fx_translation", "One employee AI economics translate to local currency using editable FX", {
        **one_employee(hire=1), "Assumptions!B11": 2,
    }, [
        ("Summary!B81", 60000, "Common-GBP first 24-month expected spending"),
        ("Summary!B82", 60000, "Common-GBP first 24-month headroom"),
        ("'Employee AI'!C26", 2500, "Per-month expected GBP spend"),
        ("'Employee AI'!C27", 2500, "Per-month unused GBP allowance"),
        ("'Employee AI'!C28", 5000, "Per-month total GBP allowance"),
        ("Summary!B32", 127500, "Funding translates back to the same GBP economics despite USD/GBP stress of 2"),
    ], "The fictional stress uses USD/GBP 2. London reports GBP and New York USD; common-GBP expected values must agree. Root also compares captured Base outputs at the approved 1.3476 FX.")

    regressions = copy.deepcopy(original)
    for case in regressions:
        case["id"] = "v1_regression_" + case["id"]
        case["inputs"].update({"Assumptions!B118": 1, "Assumptions!B119": 0})
        case["notes"] = "Original v1 expected checks unchanged; v2 regression selects historical staffing and zero employee AI allowance. V1 fixture and captured evidence remain preserved."
    return result, regressions


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, default=Path("/tmp"))
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    cases, regressions = build_cases()
    for name, data in [("m2-ai-validation-cases.json", cases), ("m2-ai-regression-cases.json", regressions)]:
        path = args.output_dir / name
        path.write_text(json.dumps(data, indent=2) + "\n")
        print(f"{path}: {len(data)} cases; {sum(len(c['checks']) for c in data)} expected arithmetic checks")


if __name__ == "__main__":
    main()
