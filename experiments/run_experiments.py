"""Run all figure-generating experiments for the paper.

Outputs saved to experiments/figures/ as PDF and PNG.
"""
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent


def run_script(name: str) -> None:
    script_path = SCRIPT_DIR / name
    print(f"[run] {script_path}")
    subprocess.check_call([sys.executable, str(script_path)])


def main() -> None:
    Path(SCRIPT_DIR / "figures").mkdir(parents=True, exist_ok=True)
    run_script("plot_alignment_error.py")
    run_script("plot_probability_mass.py")
    run_script("plot_false_association.py")
    run_script("plot_failover.py")
    print("All figures generated in experiments/figures/")


if __name__ == "__main__":
    main()
