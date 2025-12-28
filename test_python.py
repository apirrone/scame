def hello(name):
    """Say hello to someone"""
    print(f"Hello, {name}!")

def add(a, b):
    """Add two numbers"""
    return a + b

def multiply(x, y):
    """Multiply two numbers"""
    return x * y

# Test function calls - place cursor on function names and press F12
result = add(1, 2)
greeting = hello("World")
product = multiply(3, 4)

# Intentional error for diagnostics: undefined variable
# bad = undefined_var
