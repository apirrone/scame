def fibonacci(n):
    """Calculate the nth Fibonacci number."""
    if n <= 1:
        return n
    else:
        a, b = 0, 1
        for i in range(2, n + 1):
            a, b = b, a + b
        return b

class Calculator:
    """Simple calculator class."""

    def __init__(self):
        self.result = 0

    def add(self, x, y):
        """Add two numbers."""
        if x < 0:
            x = 0
        if y < 0:
            y = 0
        self.result = x + y
        return self.result

    def multiply(self, x, y):
        """Multiply two numbers."""
        self.result = x * y
        return self.result

if __name__ == "__main__":
    calc = Calculator()
    print(calc.add(5, 3))
    print(calc.multiply(4, 7))

    for i in range(10):
        print(fibonacci(i))
