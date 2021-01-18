from django.contrib import admin

from PersonalMusicBrowser.Discography import models


# Register your models here.
admin.site.register(models.Instrument)
admin.site.register(models.Band)
admin.site.register(models.Artist)
admin.site.register(models.Album)
admin.site.register(models.Discography)
admin.site.register(models.Song)
admin.site.register(models.Cover)
admin.site.register(models.Composition)
admin.site.register(models.Recording)
