import subprocess
import requests
import json
import time
import random

URI_WWW = "https://www.roblox.com/"
URI_AUTH = "https://auth.roblox.com/"
URI_FRIENDS = "https://friends.roblox.com/"
URI_ACCOUNT_SETTINGS = "https://accountsettings.roblox.com/"
URI_GAMES = "https://games.roblox.com/"
URI_USERS = "https://users.roblox.com/"

class Player:
    
    def __init__(self, cookie):
        self.roblosecurity = cookie
        self.acct_json = requests.request("GET", "https://www.roblox.com/my/account/json", cookies={".ROBLOSECURITY":self.roblosecurity}).json()
        self.csrf_token = None
        self.auth_ticket = None
        self.username = self.acct_json["Name"] 
        self.userid = self.acct_json["UserId"]
        self.valid = self.username != None

    def perform_request(self, type, suburi, method, headers = None, data = None, body = None):
        return requests.request(method, type+suburi, headers=headers, data=data, cookies={".ROBLOSECURITY":self.roblosecurity}, json=body)

    def refresh_csrf(self):
        request = self.perform_request(URI_AUTH, "v1/authentication-ticket/","POST",headers={"Referer": "https://www.roblox.com/games/3398014311/Restaurant-Tycoon-2"})
        result = request.headers["X-CSRF-TOKEN"]    
        if result != None:
            self.csrf_token = result
            return True
        return False

    def refresh_auth_ticket(self):
        request = self.perform_request(URI_AUTH, "v1/authentication-ticket/","POST",headers={"X-CSRF-TOKEN":self.csrf_token,"Referer": "https://www.roblox.com/","Content-Type": "application/json"})
        result = request.headers["rbx-authentication-ticket"]
        if result != None:
            self.auth_ticket = result
            return True
        else:
            return False

    def joinGame(self, gameid):
        print(f"Trying to join game {gameid}")
        if not self.validate():
            return False

        game = self.perform_request(URI_GAMES, f"v1/games/multiget-place-details?placeIds={gameid}", "GET")
        isgamevalid = game.text.find('"placeId"') > -1

        if not isgamevalid:
            print(f"Game {gameid} is invalid")
            return False
        
        game = game.json()[0]["name"]

        print(f"Joining game {game}")
        
        current_unix_time = int(time.time()*1000)
        spoofed_browser_tracker_id = str(random.randint(100000,1755000)) + str(random.randint(100000,900000))
        placeLauncherURL = f"https%3A%2F%2Fwww.roblox.com%2FGame%2FPlaceLauncher.ashx%3Frequest%3DRequestGame%26browserTrackerId%3D{spoofed_browser_tracker_id}%26placeId%3D{gameid}%26isPlayTogetherGame%3Dfalse%26"
        launchProtocol = f"roblox-player:1+launchmode:play+gameinfo:{self.auth_ticket}+launchtime:{current_unix_time}+placelauncherurl:{placeLauncherURL}+browsertrackerid:{spoofed_browser_tracker_id}+robloxLocale:en_us+gameLocale:en_us+channel:+LaunchExp:InApp"

        subprocess.run(["start",launchProtocol], check=True, shell=True)
        return True
    
    def joinUser(self, username):
        print(f"Trying to join {username}")
        if not self.validate():
            return self.validate()
        user = self.perform_request(URI_USERS, f"v1/usernames/users", "POST", body={"usernames":[username], "excludeBannedUsers":True})
        isuservalid = user.text.find('"data":') > -1
        if not isuservalid:
            print(f"User {username} is invalid")
            return False
        print(f"Joining {username}")
        current_unix_time = int(time.time()*1000)
        spoofed_browser_tracker_id = str(random.randint(100000,1755000)) + str(random.randint(100000,900000))
        placeLauncherURL = f"https%3A%2F%2Fwww.roblox.com%2FGame%2FPlaceLauncher.ashx%3Frequest%3DRequestFollowUser%26browserTrackerId%3D{spoofed_browser_tracker_id}%26userId%3D{user.json()['data'][0]['id']}%26joinAttemptId%3D{str(random.randint(100000,1755000))}%26joinAttemptOrigin%3DpeopleListInHomePage"
        launchProtocol = f"roblox-player:1+launchmode:play+gameinfo:{self.auth_ticket}+launchtime:{current_unix_time}+placelauncherurl:{placeLauncherURL}+browsertrackerid:{spoofed_browser_tracker_id}+robloxLocale:en_us+gameLocale:en_us+channel:+LaunchExp:InApp"

        subprocess.run(["start",launchProtocol], check=True, shell=True)
        return True

    def validate(self):
        if not self.valid:
            print("Invalid user")
            return False
        if not self.refresh_csrf():
            print("Failed to refresh csrf")
            return False
        if not self.refresh_auth_ticket():
            print("Failed to refresh auth ticket")
            return False
        return True
