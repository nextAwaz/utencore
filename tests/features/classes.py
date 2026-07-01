# Test class definition and instantiation
class Animal:
    def speak(self):
        return "?"

class Dog(Animal):
    def speak(self):
        return "Woof!"

d = Dog()
sound = d.speak()
if sound == "Woof!":
    print("PASS: classes")

print("DONE")
