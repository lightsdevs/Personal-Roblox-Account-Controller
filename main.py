from singleton import Singleton
from player import Player
from gui import render
from selenium import webdriver
from selenium.webdriver.support.wait import WebDriverWait

players = []

def main():
    Singleton.create_mutex("ROBLOX_singletonEvent") # create anti roblox mutex
    me = players[0]
    print(me.username+" "+str(me.userid))
    me.joinGame(14067600077) # join game by id
    # me.joinUser("okdokiboomer") # join user by username
    render()

def addPlayer():
    driver = webdriver.Chrome()
    driver.get("https://roblox.com/login")
    WebDriverWait(driver, 30).until(lambda d: d.current_url == "https://www.roblox.com/home")
    cookies = driver.get_cookies()
    driver.quit()
    roblosecurity = None
    for cookie in cookies:
        if cookie["name"] == ".ROBLOSECURITY":
            roblosecurity = cookie["value"]
            break
    if roblosecurity == None:
        print("Failed to get .ROBLOSECURITY cookie")
        return False
    players.append(Player(roblosecurity))
    return True

if __name__ == "__main__":
    if addPlayer():
        main()
