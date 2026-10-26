from . import base

HELLO = "hello from lib.a"
BASE = base.BASE


class A:
    def __init__(self, who):
        self.who = who

    def greet(self):
        return f"A.greet: {self.who} + {BASE}"
