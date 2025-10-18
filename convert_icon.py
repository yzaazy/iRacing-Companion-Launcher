from PIL import Image

img = Image.open('iRCL.png')
img.save('iRCL.ico', format='ICO', sizes=[(256, 256)])
print("Icon created successfully!")