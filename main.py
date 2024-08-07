from singleton import Singleton
from player import Player
from gui import render

def main():
    Singleton.create_mutex("ROBLOX_singletonEvent") # create anti roblox mutex
    me = Player("_|WARNING:-DO-NOT-SHARE-THIS.--Sharing-this-will-allow-someone-to-log-in-as-you-and-to-steal-your-ROBUX-and-items.|_what you know abotu skibidi rizz")
    print(me.username+" "+str(me.userid))
    me.joinGame(14067600077) # join game by id
    me.joinUser("okdokiboomer") # join user by username
    render()

if __name__ == "__main__":
    main()