import csv
import io
import player

class Account:
    def __init__(self, cookie, description=''):
        self.cookie = cookie
        self.player = player.Player(cookie)
        self.name = self.player.username
        self.description = description

    def __str__(self):
        return f"{self.name}  |  {self.description}"
    
    def csvstring(self):
        row = io.StringIO(newline='')
        csv.writer(row).writerow([self.name, self.cookie, self.description])
        return '\n' + row.getvalue()[:-2]
