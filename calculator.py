"""Artume Calculator — Voice-driven math expressions and unit conversion."""

import math
import re
from typing import Optional


class Calculator:
    """Voice-driven calculator for Artume OS."""

    def calculate(self, expression: str) -> str:
        """Evaluate a math expression spoken naturally."""
        # Clean up spoken math
        expr = expression.lower()
        expr = expr.replace("x", "*").replace("×", "*")
        expr = expr.replace("÷", "/").replace("divided by", "/")
        expr = expr.replace("squared", "**2").replace("cubed", "**3")
        expr = expr.replace("to the power of", "**").replace("^", "**")
        expr = expr.replace("percent", "/100").replace("pi", str(math.pi))
        expr = expr.replace("euler", str(math.e)).replace("e ", str(math.e) + " ")
        expr = re.sub(r'(\d+)\s*percent\s*of\s*(\d+)', r'(\1/100)*\2', expr)
        expr = re.sub(r'what is|calculate|compute|solve|equals', '', expr).strip()

        # Remove trailing question marks
        expr = expr.rstrip("?").strip()

        if not expr:
            return "What would you like me to calculate?"

        try:
            # Safe eval with only math functions
            allowed = {
                "abs": abs, "round": round, "min": min, "max": max,
                "sum": sum, "pow": pow, "sqrt": math.sqrt,
                "sin": math.sin, "cos": math.cos, "tan": math.tan,
                "log": math.log, "log10": math.log10, "floor": math.floor,
                "ceil": math.ceil, "pi": math.pi, "e": math.e,
            }
            result = eval(expr, {"__builtins__": {}}, allowed)
            if isinstance(result, float):
                if result == int(result):
                    result = int(result)
                else:
                    result = round(result, 6)
            return f"Result: {result}"
        except ZeroDivisionError:
            return "Cannot divide by zero."
        except Exception:
            return "I couldn't understand that calculation. Try saying it differently, like 'what is 15 percent of 200'."

    def convert(self, value: float, from_unit: str, to_unit: str) -> str:
        """Convert between units."""
        conversions = {
            # Length
            ("inches", "cm"): 2.54, ("cm", "inches"): 1/2.54,
            ("feet", "meters"): 0.3048, ("meters", "feet"): 1/0.3048,
            ("miles", "km"): 1.60934, ("km", "miles"): 1/1.60934,
            ("yards", "meters"): 0.9144, ("meters", "yards"): 1/0.9144,
            # Weight
            ("pounds", "kg"): 0.453592, ("kg", "pounds"): 1/0.453592,
            ("ounces", "grams"): 28.3495, ("grams", "ounces"): 1/28.3495,
            # Volume
            ("gallons", "liters"): 3.78541, ("liters", "gallons"): 1/3.78541,
            ("quarts", "liters"): 0.946353, ("liters", "quarts"): 1/0.946353,
            # Temperature
            ("fahrenheit", "celsius"): "f_to_c",
            ("celsius", "fahrenheit"): "c_to_f",
            # Speed
            ("mph", "kmh"): 1.60934, ("kmh", "mph"): 1/1.60934,
            # Time
            ("hours", "minutes"): 60, ("minutes", "hours"): 1/60,
            ("days", "hours"): 24, ("hours", "days"): 1/24,
        }

        key = (from_unit.lower().rstrip("s"), to_unit.lower().rstrip("s"))
        if key in conversions:
            factor = conversions[key]
            if factor == "f_to_c":
                result = (value - 32) * 5/9
                return f"{value} Fahrenheit is {round(result, 1)} Celsius."
            elif factor == "c_to_f":
                result = value * 9/5 + 32
                return f"{value} Celsius is {round(result, 1)} Fahrenheit."
            else:
                result = value * factor
                return f"{value} {from_unit} is {round(result, 2)} {to_unit}."

        return f"I don't know how to convert {from_unit} to {to_unit}."


# Global singleton
_calculator: Optional[Calculator] = None


def get_calculator() -> Calculator:
    global _calculator
    if _calculator is None:
        _calculator = Calculator()
    return _calculator
