import tkinter as tk
from tkinter import *
import csv
from tkinter import ttk
import sv_ttk
import accounts
import pywinstyles, sys
import player
from player import Player
from selenium import webdriver
from selenium.webdriver.edge.service import Service as EdgeService
from selenium.webdriver.support.wait import WebDriverWait
from webdriver_manager.microsoft import EdgeChromiumDriverManager
from selenium.common.exceptions import TimeoutException
import pandas as pd
import singleton
import os
import threading
from tkinter import messagebox


#Check for accountdata.csv, creates if doesn't exist
def check_file_exists(filename):
    # Get the directory of the current script
    script_dir = os.path.dirname(__file__)
    # Construct the full path to the file
    file_path = os.path.join(script_dir, filename)
    # Check if the file exists
    if os.path.isfile(file_path):
        print(f"The file '{filename}' exists.")
    else:
        superfile = open('accountdata.csv', 'x')
        superfile.write('username,cookie,description')
        superfile.close()

check_file_exists('accountdata.csv')


def addPlayer():
    driver = webdriver.Edge(service=EdgeService(EdgeChromiumDriverManager().install()))
    driver.get("https://www.roblox.com/login")
    try:
        # Wait for the user to finish logging in.
        # Success = URL starts with www.roblox.com but is NOT the login page.
        # We also require the URL to have settled (not be a blank/redirect interim)
        # so that we don't fire on the brief about:blank between navigations.
        def _logged_in(d):
            url = d.current_url
            return (
                url.startswith("https://www.roblox.com")
                and "/login" not in url
            )
        WebDriverWait(driver, 300).until(_logged_in)
    except TimeoutException:
        driver.quit()
        print("Login timed out")
        return False
    cookies = driver.get_cookies()
    driver.quit()
    roblosecurity = None
    for cookie in cookies:
        if cookie["name"] == ".ROBLOSECURITY":
            roblosecurity = cookie["value"]
            break
    if roblosecurity is None:
        print("Failed to get .ROBLOSECURITY cookie")
        return False
    return roblosecurity

def readdata(filea):
        accountlist = []
        with open(filea, newline='') as csv_file:
            reader = csv.reader(csv_file)
            next(reader, None)  # Skip the header.
            # Unpack the row directly in the head of the for loop.
            for username,cookie, description in reader:
                accountlist.append(accounts.Account(cookie, description))
        return accountlist

def apply_theme_to_titlebar(root):
    version = sys.getwindowsversion()

    if version.major == 10 and version.build >= 22000:
        # Set the title bar color to the background color on Windows 11 for better appearance
        pywinstyles.change_header_color(root, "#1c1c1c" if sv_ttk.get_theme() == "dark" else "#fafafa")
    elif version.major == 10:
        pywinstyles.apply_style(root, "dark" if sv_ttk.get_theme() == "dark" else "normal")

        # A hacky way to update the title bar's color on Windows 10 (it doesn't update instantly like on Windows 11)
        root.wm_attributes("-alpha", 0.99)
        root.wm_attributes("-alpha", 1)

def theme_change():
    if sv_ttk.get_theme() == 'dark':
        sv_ttk.set_theme('light')
    else:
        sv_ttk.set_theme('dark')
    apply_theme_to_titlebar(root)



root = tk.Tk()
root.title("Personal Roblox Account Controller")

first_frame = tk.Frame(root)
first_frame.pack(side=tk.LEFT, padx=20, pady=20)


button_frame = tk.Frame(first_frame)
button_frame.pack(side=tk.BOTTOM, padx=20, pady=20)

accounter = []
accountlist = tk.Listbox(first_frame)
for i in readdata('accountdata.csv'):
    accounter.append(i)
    accountlist.insert(END, str(i))
accountlist['width'] = 100
accountlist['height'] = 200
accountlist.pack(padx=20, pady=20, side=tk.LEFT, anchor='n')


#Change Theme Button
themebutton = ttk.Button(button_frame, text='Change theme light/dark', command=theme_change)
themebutton.pack(padx=20, pady=20, side=tk.LEFT)

# Set the size of the window
root.geometry("1000x500")


#join buttons
join_frame = tk.Frame(root)
join_frame.pack(side=tk.RIGHT)

#join by game
gameframe = tk.Frame(join_frame)
gameframe.pack(padx=20,pady=20,side=tk.TOP)
game_label = ttk.Label(gameframe, text='Place ID:')
game_label.pack(padx=10,pady=10,side=tk.LEFT)

game_input = tk.StringVar(root)
game_entry = ttk.Entry(gameframe, text='Place ID', textvariable=game_input)
game_entry.pack(padx=10,pady=10,side=tk.RIGHT)


def gamejoinok():
    playerokok = accounter[accountlist.curselection()[0]].player
    placeid = game_input.get()
    playerokok.joinGame(int(placeid))

gamejoin_button = ttk.Button(join_frame, text='Join by place ID', command=gamejoinok)
gamejoin_button.pack(padx=10,pady=10,side=tk.TOP)

#join by friend
friendframe = tk.Frame(join_frame)
friendframe.pack(padx=20,pady=20,side=tk.TOP)

friend_label = ttk.Label(friendframe, text='Player Username:')
friend_label.pack(padx=10,pady=10,side=tk.LEFT)

friend_input = tk.StringVar(root)
friend_entry = ttk.Entry(friendframe, text='UID', textvariable=friend_input)
friend_entry.pack(padx=10,pady=10,side=tk.RIGHT)

def playerjoinok():
    playerokok = accounter[accountlist.curselection()[0]].player
    uid = friend_input.get()
    playerokok.joinUser(uid)

friendjoin_button = ttk.Button(join_frame, text='Join by player username', command=playerjoinok)
friendjoin_button.pack(padx=10,pady=10,side=tk.TOP)

desclabel = ttk.Label(join_frame, text='Description:')
desclabel.pack(padx=10,pady=10,side=tk.TOP)
desc_text = tk.Text(join_frame)
desc_text.pack(padx=10,pady=10,side=tk.TOP)
desc_text['height'] = 10

def set_description():
    playerokok = accounter[accountlist.curselection()[0]]
    playerokok.description = desc_text.get(1.0, "end-1c")
    print(playerokok.description)
    df = pd.read_csv('accountdata.csv')
    df.loc[accounter.index(playerokok), 'description'] = playerokok.description
    df.to_csv('accountdata.csv', index=False)
    peepeeindex = accounter.index(playerokok)
    accountlist.delete(peepeeindex)
    accountlist.insert(peepeeindex, str(playerokok))


desc_button = ttk.Button(button_frame, text='Set Description', command=set_description)
desc_button.pack(padx=10,pady=10,side=tk.RIGHT)


def addaccount():
    # Disable the button while login is in progress to prevent double-clicks
    addbutton.config(state='disabled')

    def _do_add():
        try:
            cookie = addPlayer()
            if not cookie:
                def _on_fail():
                    messagebox.showerror("Add Account", "Failed to add account. Login timed out or .ROBLOSECURITY cookie could not be retrieved.")
                    addbutton.config(state='normal')
                root.after(0, _on_fail)
                return
            accoun = accounts.Account(cookie)
            with open('accountdata.csv', 'a') as dr:
                dr.write(accoun.csvstring())
            # Update UI from the main thread
            def _update_ui():
                accountlist.insert(END, str(accoun))
                accounter.append(accoun)
                addbutton.config(state='normal')
            root.after(0, _update_ui)
        except Exception as e:
            import traceback
            err = traceback.format_exc()
            print(err)
            def _on_error():
                messagebox.showerror(
                    "Add Account – Unexpected Error",
                    f"An error occurred while adding the account:\n\n{err}"
                )
                addbutton.config(state='normal')
            root.after(0, _on_error)

    threading.Thread(target=_do_add, daemon=True).start()

addbutton = ttk.Button(button_frame, text='Add Account', command=addaccount)
addbutton.pack(padx=20,pady=20,side=tk.LEFT)

def browseropenreal():
    accounter[accountlist.curselection()[0]].player.openBrowser()

browser_button = ttk.Button(button_frame, text='Open Browser', command=browseropenreal)
browser_button.pack(padx=20,pady=20,side=tk.LEFT)


def main():
    #Start event loop
    sv_ttk.set_theme('dark')
    apply_theme_to_titlebar(root)
    singleton.Singleton.create_mutex('ROBLOX_singletonEvent')
    root.mainloop()

main()
