def calculate_fibonacci(n):
    """Calculate the nth Fibonacci number using memoization."""
    if n <= 1:
        return n

    # Use memoization for better performance
    fib_cache = {0: 0, 1: 1}
    for i in range(2, n + 1):
        fib_cache[i] = fib_cache[i-1] + fib_cache[i-2]
    return fib_cache[n]

class Calculator:
    """Advanced calculator class with more features."""

    def __init__(self):
        self.result = 0
        self.history = []

    def add(self, x, y):
        """Add two numbers and store in history."""
        self.result = x + y
        self.history.append(('add', x, y, self.result))
        return self.result

    def multiply(self, x, y):
        """Multiply two numbers and store in history."""
        self.result = x * y
        self.history.append(('multiply', x, y, self.result))
        return self.result

    def subtract(self, x, y):
        """Subtract y from x."""
        self.result = x - y
        self.history.append(('subtract', x, y, self.result))
        return self.result

if __name__ == "__main__":
    calc = Calculator()
    print(calc.add(5, 3))
    print(calc.multiply(4, 7))
    print(calc.subtract(10, 3))
