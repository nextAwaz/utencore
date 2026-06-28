#!/usr/bin/env python3
"""
Python 3 几乎全部特性演示脚本
要求：Python 3.10+（为了展示 match 语句等）
运行：python3 python3_features_demo.py
"""

import sys
import os
import math
import functools
import itertools
import collections
import typing
import enum
import dataclasses
import asyncio
import contextlib
import json

# ============================================================
# 0. 脚本基本信息
# ============================================================
print(f"Python 版本: {sys.version}")
print("=" * 60)

# ============================================================
# 1. 基本数据类型与运算符
# ============================================================
print("\n1. 基本数据类型与运算符")

# 数字
an_int: int = 42
a_float: float = 3.14
a_complex: complex = 1 + 2j
print(f"整数: {an_int}, 浮点: {a_float}, 复数: {a_complex}")

# 算术运算符
print(f"除法 / : {10 / 3:.2f}")           # 浮点除法
print(f"地板除 // : {10 // 3}")           # 整除
print(f"取余 % : {10 % 3}")               # 取余
print(f"幂 ** : {2 ** 10}")               # 幂运算

# 海象运算符 (Walrus) :=   Python 3.8+
if (n := len("hello")) > 0:
    print(f"海象运算符 n := len('hello') = {n}")

# 布尔与比较
print(f"True and False: {True and False}")
print(f"链式比较 1 < 2 < 3: {1 < 2 < 3}")
print(f"身份运算符 is: {a_float is not None}")

# ============================================================
# 2. 字符串
# ============================================================
print("\n2. 字符串")

s1 = '单引号'
s2 = "双引号"
s3 = '''三引号可跨行'''
s4 = f"f-string 表达式: {an_int + 1}"    # f-string
s5 = r"原始字符串: \n不转义"               # raw string
s6 = b"bytes demo"                             # bytes literals
print(f"s1: {s1}, s2: {s2}, s3: {s3}, s4: {s4}, s5: {s5}, s6: {s6}")

# 字符串方法
print(f"upper: {'hello'.upper()}, split: {'a,b,c'.split(',')}")
print(f"连接: {'-'.join(['a','b'])}")
print(f"格式化: {'{} {}'.format('format', 'method')}")

# ============================================================
# 3. 组合数据类型
# ============================================================
print("\n3. 组合数据类型")

# 列表 (List)
lst: list = [1, 2, 3, 4]
lst.append(5)
lst.extend([6, 7])
print(f"列表: {lst}, 切片: {lst[1:4]}, 步长: {lst[::2]}")

# 列表推导式 (Comprehension)
squares = [x**2 for x in range(6) if x % 2 == 0]
print(f"列表推导式 (偶数的平方): {squares}")

# 元组 (Tuple) - 不可变
tup: tuple = (1, "two", 3.0)
print(f"元组: {tup}, 解包: a,b,c = {tup} -> {tup[0]},{tup[1]},{tup[2]}")

# 字典 (Dict) - Python 3.7+ 保持插入顺序
d: dict = {"a": 1, "b": 2}
d["c"] = 3
print(f"字典: {d}, 取值: {d.get('b')}, 合并: { {**d, 'd':4} }")

# 字典推导式
d_comp = {k: v**2 for k, v in zip('abc', range(3))}
print(f"字典推导式: {d_comp}")

# 集合 (Set) - 无序、不重复
s: set = {1, 2, 3, 3}
print(f"集合: {s}, 集合推导式: { {x for x in 'abracadabra'} }")

# 解包操作 *
first, *rest = [1, 2, 3, 4]
print(f"解包 first, *rest = [1,2,3,4]: first={first}, rest={rest}")

# ============================================================
# 4. 控制流
# ============================================================
print("\n4. 控制流")

# if-elif-else
x = 10
if x > 5:
    print("x > 5")
elif x == 5:
    print("x == 5")
else:
    print("x < 5")

# 三元表达式
y = "even" if x % 2 == 0 else "odd"
print(f"三元表达式: {y}")

# for 循环
for i, v in enumerate(['a', 'b', 'c']):
    print(f"  enumerate: {i}: {v}")

# while 循环
count = 0
while count < 2:
    print(f"  while count={count}")
    count += 1

# 结构化模式匹配 (Python 3.10+)
def http_status(code):
    match code:
        case 200:
            return "OK"
        case 404:
            return "Not Found"
        case 500:
            return "Internal Server Error"
        case _:
            return "Unknown"

print(f"match 语句: 200 -> {http_status(200)}, 403 -> {http_status(403)}")

# ============================================================
# 5. 函数与参数
# ============================================================
print("\n5. 函数与参数")

def greet(name, greeting="Hello", *, punctuation="!"):
    """带有类型提示、默认值、关键字限定参数的函数"""
    return f"{greeting}, {name}{punctuation}"

print(f"函数调用: {greet('World')}")
print(f"函数注释: {greet.__annotations__}")

# 可变参数 *args, **kwargs
def var_args(*args, **kwargs):
    print(f"  *args: {args}")
    print(f"  **kwargs: {kwargs}")

var_args(1, 2, a=3, b=4)

# Lambda 表达式
square = lambda x: x**2
print(f"Lambda: square(5) = {square(5)}")

# 高阶函数 map, filter, reduce
nums = [1, 2, 3, 4]
print(f"map: {list(map(lambda x: x*2, nums))}")
print(f"filter: {list(filter(lambda x: x>2, nums))}")
print(f"functools.reduce: {functools.reduce(lambda a,b: a+b, nums)}")

# 闭包 (Closure)
def make_multiplier(factor):
    def multiplier(x):
        return x * factor
    return multiplier

times3 = make_multiplier(3)
print(f"闭包: times3(5) = {times3(5)}")

# ============================================================
# 6. 装饰器 (Decorator)
# ============================================================
print("\n6. 装饰器")

def logger(func):
    @functools.wraps(func)
    def wrapper(*args, **kwargs):
        print(f"  调用 {func.__name__} with args={args}, kwargs={kwargs}")
        result = func(*args, **kwargs)
        print(f"  返回 {result}")
        return result
    return wrapper

@logger
def add(a, b):
    return a + b

print(f"装饰器结果: {add(3, 4)}")

# 带参数的装饰器
def repeat(n):
    def decorator(func):
        @functools.wraps(func)
        def wrapper(*args, **kwargs):
            for _ in range(n):
                func(*args, **kwargs)
        return wrapper
    return decorator

@repeat(2)
def say_hi():
    print("  Hi!")

print("带参数的装饰器 (repeat 2):")
say_hi()

# ============================================================
# 7. 类与面向对象
# ============================================================
print("\n7. 类与面向对象")

class Animal:
    """基类演示"""
    kingdom = "Animalia"   # 类变量

    def __init__(self, name):
        self.name = name   # 实例变量

    def speak(self):
        raise NotImplementedError("子类必须实现 speak 方法")

    @classmethod
    def get_kingdom(cls):
        return cls.kingdom

    @staticmethod
    def info():
        return "这是一个动物类"

class Dog(Animal):
    def __init__(self, name, breed):
        super().__init__(name)
        self.breed = breed

    def speak(self):
        return "Woof!"

    # 属性 (Property)
    @property
    def description(self):
        return f"{self.name} is a {self.breed}"

    @description.setter
    def description(self, value):
        self.name, self.breed = value.split()

# 创建实例
dog = Dog("Buddy", "Golden Retriever")
print(f"类变量: {Dog.get_kingdom()}")
print(f"静态方法: {Animal.info()}")
print(f"实例属性: {dog.name}, speak: {dog.speak()}")
print(f"property: {dog.description}")

# 魔术方法 (Dunder)
class Vector:
    def __init__(self, x, y):
        self.x = x
        self.y = y

    def __repr__(self):
        return f"Vector({self.x}, {self.y})"

    def __add__(self, other):
        return Vector(self.x + other.x, self.y + other.y)

    def __eq__(self, other):
        return self.x == other.x and self.y == other.y

    def __bool__(self):
        return self.x != 0 or self.y != 0

v1 = Vector(1, 2)
v2 = Vector(3, 4)
print(f"魔术方法: {v1} + {v2} = {v1 + v2}")
print(f"__eq__: {v1 == Vector(1,2)}")
print(f"__bool__: bool({v1}) = {bool(v1)}")

# 数据类 (dataclass) Python 3.7+
@dataclasses.dataclass
class Point:
    x: float
    y: float
    label: str = ""

p = Point(1.0, 2.0, "A")
print(f"数据类: {p}, x={p.x}")

# 枚举 (Enum)
class Color(enum.Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

print(f"枚举: {Color.RED}, 值: {Color.RED.value}")

# ============================================================
# 8. 迭代器与生成器
# ============================================================
print("\n8. 迭代器与生成器")

# 可迭代对象与迭代器
my_list = [1, 2, 3]
it = iter(my_list)
print(f"迭代器: {next(it)}, {next(it)}")

# 生成器函数 (Generator)
def countdown(n):
    while n > 0:
        yield n
        n -= 1

print("生成器 countdown(3):", list(countdown(3)))

# 生成器表达式
gen_exp = (x**2 for x in range(5))
print(f"生成器表达式: {next(gen_exp)}, {list(gen_exp)}")

# itertools 模块
print(f"itertools.chain: {list(itertools.chain('AB', 'CD'))}")
print(f"itertools.cycle (前5): {list(itertools.islice(itertools.cycle('AB'), 5))}")

# ============================================================
# 9. 异常处理
# ============================================================
print("\n9. 异常处理")

try:
    result = 10 / 0
except ZeroDivisionError as e:
    print(f"捕获异常: {type(e).__name__}: {e}")
else:
    print("没有异常")
finally:
    print("finally 块总会执行")

# 自定义异常
class MyError(Exception):
    pass

try:
    raise MyError("自定义错误")
except MyError as e:
    print(f"自定义异常: {e}")

# 上下文管理器 (with 语句)
class ManagedFile:
    def __init__(self, name):
        self.name = name
    def __enter__(self):
        print(f"打开文件 {self.name}")
        return self
    def __exit__(self, exc_type, exc_val, exc_tb):
        print("关闭文件")
        return False  # 不抑制异常

with ManagedFile("test.txt") as f:
    print("  在上下文中执行")

# 使用 contextlib 简化
from contextlib import contextmanager

@contextmanager
def managed_resource(name):
    print(f"获取资源 {name}")
    yield name
    print(f"释放资源 {name}")

with managed_resource("数据库连接") as res:
    print(f"  使用 {res}")

# ============================================================
# 10. 文件 I/O 与 JSON
# ============================================================
print("\n10. 文件 I/O 与 JSON")

# 写入并读取文件
with open("demo.txt", "w", encoding="utf-8") as f:
    f.write("Hello, Python3!\n第二行")

with open("demo.txt", "r", encoding="utf-8") as f:
    content = f.read()
    print(f"文件内容: {content.strip()}")

# JSON 序列化/反序列化
data = {"name": "Alice", "age": 30, "languages": ["Python", "Go"]}
json_str = json.dumps(data, indent=2)
print(f"JSON 序列化:\n{json_str}")
restored = json.loads(json_str)
print(f"JSON 反序列化: {restored}")

# 清理临时文件
os.remove("demo.txt")

# ============================================================
# 11. 类型提示 (Type Hints)
# ============================================================
print("\n11. 类型提示")

def typed_func(a: int, b: str) -> bool:
    return isinstance(a, int) and isinstance(b, str)

print(f"类型提示函数: {typed_func(5, 'test')}")
print(f"类型注释: {typed_func.__annotations__}")

# 泛型提示 (需要 typing 模块)
T = typing.TypeVar('T')

def first_element(lst: typing.List[T]) -> T:
    return lst[0]

print(f"泛型函数: first_element([10,20]) = {first_element([10,20])}")

# ============================================================
# 12. 异步编程 (asyncio)
# ============================================================
print("\n12. 异步编程")

async def async_task(name, delay):
    print(f"  开始 {name}")
    await asyncio.sleep(delay)
    print(f"  完成 {name}")
    return f"{name} 结果"

async def main():
    # 并发执行多个协程
    results = await asyncio.gather(
        async_task("A", 0.2),
        async_task("B", 0.1)
    )
    print(f"  异步结果: {results}")

# 运行异步主函数
asyncio.run(main())

# ============================================================
# 13. 其他高级特性
# ============================================================
print("\n13. 其他高级特性")

# 元类 (Metaclass) 简单演示
class Meta(type):
    def __new__(cls, name, bases, dct):
        dct['meta_attr'] = '由元类添加'
        return super().__new__(cls, name, bases, dct)

class MyClass(metaclass=Meta):
    pass

print(f"元类属性: {MyClass.meta_attr}")

# 弱引用 (weakref) - 仅演示导入，实际使用需创建对象
import weakref
print("weakref 模块已导入（可创建弱引用）")

# 描述符 (Descriptor) 协议
class PositiveNumber:
    def __set_name__(self, owner, name):
        self.name = name
    def __get__(self, obj, objtype=None):
        return obj.__dict__.get(self.name, 0)
    def __set__(self, obj, value):
        if value < 0:
            raise ValueError("必须是正数")
        obj.__dict__[self.name] = value

class Order:
    price = PositiveNumber()

    def __init__(self, price):
        self.price = price

order = Order(100)
print(f"描述符: order.price = {order.price}")

# 部分函数 (functools.partial)
from functools import partial
basetwo = partial(int, base=2)
print(f"partial int base2: {basetwo('1010')}")

# 缓存 (lru_cache)
@functools.lru_cache(maxsize=128)
def fibonacci(n):
    if n < 2:
        return n
    return fibonacci(n-1) + fibonacci(n-2)

print(f"fibonacci(10) (带缓存): {fibonacci(10)}")

# 抽象基类
from abc import ABC, abstractmethod

class Shape(ABC):
    @abstractmethod
    def area(self):
        pass

class Circle(Shape):
    def __init__(self, radius):
        self.radius = radius
    def area(self):
        return math.pi * self.radius ** 2

c = Circle(2)
print(f"ABC 抽象类: Circle area = {c.area():.2f}")

# ============================================================
# 14. 模块与包
# ============================================================
print("\n14. 模块与包")

# 显示搜索路径
print(f"当前模块: {__name__}")
print(f"文件路径: {__file__}")

# 相对导入示例（在本脚本中不能实际使用，仅演示语法）
print("# 相对导入语法: from . import module (仅在包内模块可用)")

# __all__ 演示
__all__ = ['greet', 'Dog', 'Vector']   # 限制 from module import * 时导出的名字

# ============================================================
# 15. 内置函数与常识
# ============================================================
print("\n15. 常用内置函数")

print(f"isinstance(42, int): {isinstance(42, int)}")
print(f"hasattr(dog, 'speak'): {hasattr(dog, 'speak')}")
print(f"getattr(dog, 'name'): {getattr(dog, 'name')}")
print(f"callable(print): {callable(print)}")
print(f"zip('AB', [1,2]): {list(zip('AB', [1,2]))}")
print(f"reversed([1,2,3]): {list(reversed([1,2,3]))}")
print(f"sorted([3,1,2]): {sorted([3,1,2])}")
print(f"all([True, 1]): {all([True, 1])}, any([0, False, 3]): {any([0, False, 3])}")

# 格式化输出
print("结束演示。")

# ============================================================
# 脚本结束
# ============================================================