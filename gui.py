import pygame
from player import Player
import sys

def render():
    pygame.init()
    size = (200, 200)
    screen = pygame.display.set_mode(size)
    pygame.display.set_caption('Multi Roblox')

    image = pygame.image.load('mr.png')

    running = True
    while running:
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                running = False

        screen.fill((0, 0, 0))
        screen.blit(image, (0, 0))
        pygame.display.flip()
    pygame.quit()
    sys.exit()
